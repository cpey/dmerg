# dmerg

Command line utility to merge the standard output and the kernel ring buffer.

By default *dmerg* collects kernel logs from the *systemd* journal. Only members of the groups "systemd-journal", "adm", and "wheel" can read all journal files. Alternatively, *dmerg* can be told to use *dmesg* output. In such case, beware of *dmesg* time not being in sync with the system clock. Unless otherwise told, *dmerg* picks kernel log messages with a date that is newer than the time of its execution. *dmesg* messages showing a clock that is late, will delay the starting point of kernel message collection.

## Options

```
~$ ./dmerg -h
dmerg 0.1.0

USAGE:
    dmerg [FLAGS] [OPTIONS]

FLAGS:
    -c, --console-off    Do not write to the standard output
    -d, --dmesg          Use dmesg instead of journald
    -f, --full           Include full kernel log output
    -h, --help           Prints help information
    -V, --version        Prints version information

OPTIONS:
    -o, --output <output>    Write output to <output> instead of a random file
```

## Example

```
~$ for i in {1..10}; do echo $i; sleep 1; done | ./dmerg
2021-09-17T18:41:02.668895+0000 1
2021-09-17T18:41:03.663459+0000 2
2021-09-17T18:41:04.672363+0000 3
2021-09-17T18:41:05.055183+0000 test kernel: Unloaded Memory Tracer
2021-09-17T18:41:05.079075+0000 test kernel: Memory Tracer
2021-09-17T18:41:05.674361+0000 4
2021-09-17T18:41:06.674690+0000 5
2021-09-17T18:41:06.350965+0000 test kernel: Unloaded Memory Tracer
2021-09-17T18:41:06.375967+0000 test kernel: Memory Tracer
2021-09-17T18:41:07.678787+0000 6
2021-09-17T18:41:07.343006+0000 test kernel: Unloaded Memory Tracer
2021-09-17T18:41:07.368014+0000 test kernel: Memory Tracer
2021-09-17T18:41:08.683783+0000 7
^C
+ Output written to dmerged.Td8j7omccwVump41

~$ cat dmerged.Td8j7omccwVump41
2021-09-17T18:41:02.668895+0000 1
2021-09-17T18:41:03.663459+0000 2
2021-09-17T18:41:04.672363+0000 3
2021-09-17T18:41:05.055183+0000 test kernel: Unloaded Memory Tracer
2021-09-17T18:41:05.079075+0000 test kernel: Memory Tracer
2021-09-17T18:41:05.674361+0000 4
2021-09-17T18:41:06.350965+0000 test kernel: Unloaded Memory Tracer
2021-09-17T18:41:06.375967+0000 test kernel: Memory Tracer
2021-09-17T18:41:06.674690+0000 5
2021-09-17T18:41:07.343006+0000 test kernel: Unloaded Memory Tracer
2021-09-17T18:41:07.368014+0000 test kernel: Memory Tracer
2021-09-17T18:41:07.678787+0000 6
2021-09-17T18:41:08.683783+0000 7
```

