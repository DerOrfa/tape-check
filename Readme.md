# md5check

A simple tool to check md5 checksums on systems with limited primary and slow secondary storage like tape storage.

It will read checksum files created by `md5sum` and from there try to open (and thus trigger recall of) and check as many files as possible within a given size limit.
An optional release command can be given, to be executed after a file was completely red.

## Examples

### Check in the current directory with defaults
This will try to read the file `md5sum` in the current directory and check files therein.
```shell
md5check
```

### Check multiple directories
```shell
md5check 1902??/md5sum --limit 700 --release "ivdfile --release"
```
This will try to read the files `md5sum` in the subdirectories fitting the pattern `1902??` in the current directory and check files therein.
- at no point in time will be more than 700G active in the primary filesystem
- the command `ivdfile --release` will be called on finished files


## fully static linked build

In case of problems with GLIBC on ancient Linuxes try a fully static build.

```shell
rustup target add x86_64-unknown-linux-musl
cargo build --target x86_64-unknown-linux-musl
```

