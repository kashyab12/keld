#!/usr/bin/env python3
"""Independent FirstFrame driver: a separate process that talks to Sable only
through the input FIFO and the presented frame files.

Positive run: frame 1 must be red, a random-x tap must produce frame 2 in blue
(left half) or green (right half), and quit must exit 0. Negative controls:
no input means no frame 2; malformed input, a forged framebuffer range and a
missing provider export must each reject with Sable's error exit (125).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import tempfile
import time

WIDTH, HEIGHT = 390, 844
RED, BLUE, GREEN = bytes([0, 0, 255, 255]), bytes([255, 0, 0, 255]), bytes([0, 255, 0, 255])
ROOT = Path(__file__).resolve().parent.parent


def command(o, frames, fifo, uikit=None):
    d = o.dir
    libs = [uikit or d / 'UIKit.dylib', d / 'Foundation.dylib', d / 'libobjc.dylib', d / 'libSystem.dylib']
    cmd = [str(o.sable), 'run', str(d / o.app), o.engine, '--main']
    for lib in libs:
        cmd += ['--lib', str(lib)]
    return cmd + ['--info-plist', str(o.plist), '--input', str(fifo), '--frames', str(frames)]


def wait_frame(frames, n, timeout, process):
    sha = frames / f'{n:04d}.sha256'
    deadline = time.monotonic() + timeout
    while not sha.exists():
        if process.poll() is not None or time.monotonic() > deadline:
            return None
        time.sleep(0.01)
    pixels = (frames / f'{n:04d}.rgba').read_bytes()
    assert len(pixels) == WIDTH * HEIGHT * 4, len(pixels)
    assert hashlib.sha256(pixels).hexdigest() == sha.read_text().strip(), 'frame hash mismatch'
    return pixels


def uniform(pixels, color):
    """Every pixel equals color; also spot-check an interior pixel explicitly."""
    interior = ((HEIGHT // 2) * WIDTH + WIDTH // 2) * 4
    return pixels[interior:interior + 4] == color and pixels == color * (WIDTH * HEIGHT)


class Session:
    def __init__(self, o, uikit=None):
        self.dir = Path(tempfile.mkdtemp(prefix='firstframe-', dir=o.work))
        self.frames = self.dir / 'frames'
        fifo = self.dir / 'input'
        os.mkfifo(fifo)
        self.stderr = open(self.dir / 'stderr.txt', 'wb')
        self.process = subprocess.Popen(command(o, self.frames, fifo, uikit), stdout=subprocess.DEVNULL, stderr=self.stderr)
        # Opening the write end blocks until Sable opens the read end, or fails
        # if Sable already exited during linking.
        deadline = time.monotonic() + 20
        self.input = None
        while self.input is None and time.monotonic() < deadline and self.process.poll() is None:
            try:
                self.input = os.open(fifo, os.O_WRONLY | os.O_NONBLOCK)
            except OSError:
                time.sleep(0.01)
        if self.input is not None:
            os.set_blocking(self.input, True)

    def send(self, event):
        os.write(self.input, (json.dumps(event, separators=(',', ':')) + '\n').encode())

    def finish(self, timeout=30):
        if self.input is not None:
            os.close(self.input)
        try:
            code = self.process.wait(timeout)
        except subprocess.TimeoutExpired:
            self.process.kill()
            code = self.process.wait()
        self.stderr.close()
        return code, (self.dir / 'stderr.txt').read_text(errors='replace')


def positive(o, rng):
    s = Session(o)
    frame1 = wait_frame(s.frames, 1, 60, s.process)
    assert frame1 is not None, 'no frame 1'
    assert uniform(frame1, RED), 'frame 1 is not red'
    x = rng.uniform(0, WIDTH - 1e-6)
    y = rng.uniform(0, HEIGHT - 1e-6)
    s.send({'type': 'touch', 'phase': 'began', 'x': round(x, 3), 'y': round(y, 3)})
    frame2 = wait_frame(s.frames, 2, 60, s.process)
    expected = BLUE if round(x, 3) < WIDTH / 2 else GREEN
    assert frame2 is not None, 'no frame 2'
    assert uniform(frame2, expected), 'frame 2 has the wrong color'
    s.send({'type': 'quit'})
    code, err = s.finish()
    assert code == 0, err
    return {'x': round(x, 3), 'y': round(y, 3), 'expected': 'blue' if expected == BLUE else 'green',
            'frame1_sha256': hashlib.sha256(frame1).hexdigest(), 'frame2_sha256': hashlib.sha256(frame2).hexdigest(),
            'stderr': err.strip().splitlines()[-1]}


def no_input(o):
    s = Session(o)
    assert wait_frame(s.frames, 1, 60, s.process) is not None
    assert wait_frame(s.frames, 2, 2, s.process) is None, 'frame 2 appeared without input'
    s.send({'type': 'quit'})
    code, err = s.finish()
    assert code == 0, err
    return 'no frame 2 within 2 s without input; quit exits 0'


def rejected(o, name, uikit=None, line=None, expect=''):
    s = Session(o, uikit)
    if s.input is not None and line is not None:
        wait_frame(s.frames, 1, 60, s.process)
        os.write(s.input, line)
    code, err = s.finish()
    assert code == 125, f'{name}: exit {code}: {err}'
    assert expect in err, f'{name}: {err}'
    return err.strip().splitlines()[-1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--sable', type=Path, default=Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')) / 'release/sable')
    p.add_argument('--dir', type=Path, default=ROOT / 'artifacts/firstframe')
    p.add_argument('--app', default='FirstFrame-Os')
    p.add_argument('--plist', type=Path, default=ROOT / 'modules/sable/fixtures/apps/FirstFrame/Info.bplist')
    p.add_argument('--engine', choices=['interp', 'jit'], default='interp')
    p.add_argument('--seed', type=int, default=None)
    p.add_argument('--runs', type=int, default=3)
    o = p.parse_args()
    o.work = Path(tempfile.mkdtemp(prefix='firstframe-driver-', dir=os.environ.get('MAKO_THREAD_DATA_DIR')))
    seed = o.seed if o.seed is not None else random.SystemRandom().randrange(2**32)
    rng = random.Random(seed)
    receipt = {'engine': o.engine, 'app': o.app, 'seed': seed,
               'positive': [positive(o, rng) for _ in range(o.runs)],
               'no_input': no_input(o),
               'malformed': rejected(o, 'malformed', line=b'{"type":"touch","phase":"began","x":1e999,"y":3}\n', expect='malformed input event'),
               'forged_framebuffer': rejected(o, 'forged', uikit=o.dir / 'UIKit-forged.dylib', expect='guest memory'),
               'missing_export': rejected(o, 'missing', uikit=o.dir / 'UIKit-missing.dylib', expect='_UIApplicationMain')}
    print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()
