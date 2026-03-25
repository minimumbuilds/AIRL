/*
 * airl_rt_process.c - Process spawning for the AIRL runtime
 *
 * Implements shell-exec using POSIX fork/exec with pipe() for
 * stdout/stderr capture.
 */

#include "airl_rt.h"
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>

/* ------------------------------------------------------------------ */
/*  Helper: read all data from a file descriptor into a malloc'd buf  */
/* ------------------------------------------------------------------ */

static char *read_fd_all(int fd, size_t *out_len) {
    size_t cap = 1024;
    size_t len = 0;
    char *buf = (char *)malloc(cap);

    while (1) {
        if (len + 256 > cap) {
            cap *= 2;
            buf = (char *)realloc(buf, cap);
        }
        ssize_t n = read(fd, buf + len, cap - len);
        if (n <= 0) break;
        len += (size_t)n;
    }
    buf[len] = '\0';
    *out_len = len;
    return buf;
}

/* ------------------------------------------------------------------ */
/*  Helper: wrap in Ok/Err variants                                   */
/* ------------------------------------------------------------------ */

static RtValue *proc_ok(RtValue *inner) {
    RtValue *tag = airl_str("Ok", 2);
    RtValue *result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

static RtValue *proc_err(const char *msg) {
    RtValue *tag = airl_str("Err", 3);
    RtValue *inner = airl_str(msg, strlen(msg));
    RtValue *result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

/* ------------------------------------------------------------------ */
/*  shell-exec(command, args) -> Result[Map, String]                  */
/* ------------------------------------------------------------------ */

RtValue *airl_shell_exec(RtValue *command, RtValue *args) {
    if (command->tag != RT_STR) {
        return proc_err("shell-exec: command must be string");
    }
    if (args->tag != RT_LIST) {
        return proc_err("shell-exec: args must be list");
    }

    /* Build NULL-terminated argv array: [command, arg0, arg1, ..., NULL] */
    size_t argc = args->data.list.len;
    char **argv = (char **)malloc((argc + 2) * sizeof(char *));

    /* Null-terminate command string */
    char *cmd = (char *)malloc(command->data.s.len + 1);
    memcpy(cmd, command->data.s.ptr, command->data.s.len);
    cmd[command->data.s.len] = '\0';
    argv[0] = cmd;

    size_t args_off = args->data.list.offset;
    for (size_t i = 0; i < argc; i++) {
        RtValue *arg = args->data.list.items[args_off + i];
        if (arg->tag != RT_STR) {
            /* Clean up on error */
            for (size_t j = 0; j <= i; j++) free(argv[j]);
            free(argv);
            return proc_err("shell-exec: all args must be strings");
        }
        char *s = (char *)malloc(arg->data.s.len + 1);
        memcpy(s, arg->data.s.ptr, arg->data.s.len);
        s[arg->data.s.len] = '\0';
        argv[i + 1] = s;
    }
    argv[argc + 1] = NULL;

    /* Create pipes for stdout and stderr */
    int stdout_pipe[2];
    int stderr_pipe[2];

    if (pipe(stdout_pipe) != 0 || pipe(stderr_pipe) != 0) {
        for (size_t i = 0; i <= argc; i++) free(argv[i]);
        free(argv);
        return proc_err("shell-exec: pipe() failed");
    }

    pid_t pid = fork();
    if (pid < 0) {
        close(stdout_pipe[0]); close(stdout_pipe[1]);
        close(stderr_pipe[0]); close(stderr_pipe[1]);
        for (size_t i = 0; i <= argc; i++) free(argv[i]);
        free(argv);
        return proc_err("shell-exec: fork() failed");
    }

    if (pid == 0) {
        /* Child process */
        close(stdout_pipe[0]);  /* close read end */
        close(stderr_pipe[0]);
        dup2(stdout_pipe[1], STDOUT_FILENO);
        dup2(stderr_pipe[1], STDERR_FILENO);
        close(stdout_pipe[1]);
        close(stderr_pipe[1]);

        execvp(argv[0], argv);

        /* If execvp returns, it failed */
        fprintf(stderr, "command not found: %s", argv[0]);
        _exit(127);
    }

    /* Parent process */
    close(stdout_pipe[1]);  /* close write ends */
    close(stderr_pipe[1]);

    /* Read stdout and stderr */
    size_t stdout_len, stderr_len;
    char *stdout_buf = read_fd_all(stdout_pipe[0], &stdout_len);
    char *stderr_buf = read_fd_all(stderr_pipe[0], &stderr_len);

    close(stdout_pipe[0]);
    close(stderr_pipe[0]);

    /* Wait for child */
    int status;
    waitpid(pid, &status, 0);

    int exit_code;
    if (WIFEXITED(status)) {
        exit_code = WEXITSTATUS(status);
    } else {
        exit_code = -1;
    }

    /* Free argv */
    for (size_t i = 0; i <= argc; i++) free(argv[i]);
    free(argv);

    /* Build result map: {"stdout": ..., "stderr": ..., "exit-code": ...} */
    RtValue *m = airl_map_new();

    RtValue *k1 = airl_str("stdout", 6);
    RtValue *v1 = airl_str(stdout_buf, stdout_len);
    RtValue *m2 = airl_map_set(m, k1, v1);
    airl_value_release(m); airl_value_release(k1); airl_value_release(v1);

    RtValue *k2 = airl_str("stderr", 6);
    RtValue *v2 = airl_str(stderr_buf, stderr_len);
    RtValue *m3 = airl_map_set(m2, k2, v2);
    airl_value_release(m2); airl_value_release(k2); airl_value_release(v2);

    RtValue *k3 = airl_str("exit-code", 9);
    RtValue *v3 = airl_int((int64_t)exit_code);
    RtValue *m4 = airl_map_set(m3, k3, v3);
    airl_value_release(m3); airl_value_release(k3); airl_value_release(v3);

    free(stdout_buf);
    free(stderr_buf);

    return proc_ok(m4);
}
