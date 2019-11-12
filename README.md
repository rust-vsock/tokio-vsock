# Tokio-vsock

Asynchronous Virtio socket support for Rust. The implementation is 
based off of Tokio and Mio's TCPListener and TCPStream interfaces.

Tokio-vsock is for the most part pre-alpha quality, so there is probably 
sharp edges. Please test it thoroughly before using in production. Happy to receive
pull requests and issues.

## Usage

Refer to the crate documentation.

## Testing

### Prerequisites

You will need a recent qemu-system-x86_64 build in your path.

### Host

Setup the required Virtio kernel modules:

```
make kmod
```

Start the test vm, you can shutdown the vm with the keyboard shortcut ```Ctrl+A``` and then ```x```:

```
make vm
```

### Tests

Run the test suite with:

```
make check
```

## TODO

* More detailed documentation and examples.
* Further test coverage, including long running tests.