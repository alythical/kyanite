# Kyanite

Languages are cool! Compilers are cool! How do they work? That's why this project exists: Kyanite is a statically-typed, compiled programming language to learn more about how PLs are created. The current (only) backend is LLVM, but I may pursue adding a custom backend in the future.

## Documentation

TODO (including a language reference)

In the meantime, there are some working samples to try out in the `examples/` folder.

## Explore

**Note:** Only macOS is tested; there are unresolved issues on Linux related to linking `libkyanite_builtins` (the shared library for hacking together builtin functions for debug purposes)

The project is a standard Rust workspace; the entrypoint is found within `kyanite-cli`. The recommended way to run `.kya` programs is with Nix:

```sh
nix develop -c cargo run -- run <filename>
```

Other methods may also work, but are untested. Note that LLVM and clang are additional required dependencies.