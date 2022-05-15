## SIMU
SIMU is a solution to provide a web-interface for filesystems that applies systemic authentication and POSIX access control lists.

## Install

Requirements:
- Working C compiler environment (e.g. build-essential metapackage on Debian)
- Rust, minimum supported Rust version 1.57.0.
- PAM used for authentication, and has development headers installed (e.g. libpam0g-dev package on Debian).

Build the Rust project
```
$ cargo build --release
```
LTO builds can be used as well, which can be built using `cargo build --profile release-lto`
The system needs to be either run as root or the `simu_suid_helper` binary needs to be capable of `setuid(0)` and arbitrary `setgid`.
Commonly this is done with making the root account the file owner and adding the SUID bit, like so:
```
# chown root:root path/to/simu_suid_helper
# chmod u+s path/to/simu_suid_helper
```

An example deployment scheme is provided in the `examples/` directory, which contains a sample systemd unit, example NGINX reverse proxy site configuration and an installation script.
This script will install this server on a Debian-like server, by placing the binaries to `/usr/local/bin` and assigning the SUID bit and installing the sample systemd service to start this server to listen on an UNIX domain socket at `/var/run/simu/simu.socket`.

An example NGINX reverse proxy site provided is configured to fit the SIMU application started using the sample systemd service, by proxying to the UNIX domain socket at `/var/run/simu/simu.socket`.

## Configure

SIMU application is configured through environment variables.
Following are the configurable variables.

### SIMU_TEMPLATES
This variable is used to provide the application with the location of the templates used to display directory listings and errors to the client.
Defaults to `$PWD/static/templates`

### SIMU_BIND
This variable defines where the SIMU application binds to.
You can provide either UNIX domain socket paths or TCP addresses and ports with the format of `unix:/path/to/socket` or `tcp:0.0.0.0:8088` respectively.
Defaults to `tcp:0.0.0.0:8080`.

### RUST_LOG
This variable sets the log-level of the application, at default level only fatal information is outputted.
Possible values: error, warn, info, debug, trace.
Levels after info are great in detail and are very noisy.
They are usually only used to diagnose issues within the system.


## Testing

SIMU system has an accompanying full system test using Nix package manager and NixOS.
Due to the system requiring elevated permissions testing this system is cumbersome.
NixOS-based tests make testing using virtual machines easier and this approach is taken to test this system.

Nix package manager and a Linux-based system is required to perform these tests.
Less common CPU architectures, such as ARM-based or MIPS-based architectures may have issues with these due to lesser Nix project's support.

These tests requires a Nix-based package for this application, which is provided in `simu.nix` file.
This package is currently missing templates, and thus directory display will not work on this server.

These tests are built using the `test-vm.nix` file, which describes two VMs, one for the server, and one for the client.
These tests can be run using the command `nix-build test-vm.nix`.
The client VM will attempt using the server with the preconfigured credentials.
