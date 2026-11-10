// Original MIT research helper. Runs only on an owned disposable Linux host/guest.
#define _GNU_SOURCE
#include <grp.h>
#include <stdio.h>
#include <sys/prctl.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc < 2 || setgroups(0, NULL) || setgid(1000) || setuid(1000) ||
        prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)) {
        perror("drop privileges"); return 125;
    }
    execv(argv[1], argv + 1);
    perror("exec"); return 125;
}
