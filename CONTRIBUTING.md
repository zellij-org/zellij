# Contributing to Zellij

Thanks for considering to contribute to Zellij!

**First**: if you're unsure of anything, feel free to ask on our [Discord
server](https://discord.gg/MHV3n76PDq). We're a friendly and welcoming bunch!

# Code of Conduct

Before contributing please read our [Code of Conduct](CODE_OF_CONDUCT.md) which
all contributors are expected to adhere to.

## Building
To build Zellij, we're using cargo-make – you can install it by running `cargo
install --force cargo-make`. 

To edit our manpage, the mandown crate (`cargo install
mandown`) is used and the work is done on a markdown file in docs/MANPAGE.md.

Here are some of the commands currently supported by the build system:

```sh
# Format code, build, then run tests and clippy
cargo make
# You can also perform these actions individually
cargo make format
cargo make build
cargo make test
# Run Zellij (optionally with additional arguments)
cargo make run
cargo make run -l strider
# Run Clippy (potentially with additional options)
cargo make clippy
cargo make clippy -W clippy::pedantic
# Install Zellij to some directory
cargo make install /path/of/zellij/binary
# Publish the zellij and zellij-tile crates
cargo make publish
# Update manpage
cargo make manpage
```

To run `install` or `publish`, you'll need the package `binaryen` in the
version `wasm-opt --version` > 97, for it's command `wasm-opt`.

To run `test`, you will need the package `pkg-config` and a version of `openssl`.

## Running the end-to-end tests
Zellij includes some end to end tests which test the whole application as a black-box from the outside.
These tests work by running a docker container which contains the Zellij binary, connecting to it via ssh, sending some commands and comparing the output received against predefined snapshots.

To run these tests locally, you'll need to have both `docker` and `docker-compose` installed.
Once you do, in the repository root:
1. `docker-compose up -d` will start up the docker container
2. `cargo make build-e2e` will build the generic linux executable of Zellij in the target folder, which is shared with the container
3. `cargo make e2e-test` will run the tests

To re-run the tests after you've changed something in the code base, be sure to repeat steps 2 and 3.

## Looking for something to work on?

If you are new contributor to `Zellij` going through
[beginners][good-first-issue] should be a good start or you can join our public
[Discord server][discord-invite-link], we would be happy to help finding
something interesting to work on and guide through.

[discord-invite-link]: https://discord.gg/feHDHahHCz [good-first-issue]:
https://github.com/zellij-org/zellij/labels/good%20first%20issue

## Filing Issues

Bugs and enhancement suggestions are tracked as GitHub issues.

### Lacking API for plugin in Zellij?

If you have a plugin idea, but Zellij still doesn't have API required to make
the plugin consider opening [an issue][plugin-issue] and describing your
requirements.

[plugin-issue]:
https://github.com/zellij-org/zellij/issues/new?assignees=&labels=plugin%20system

### How Do I Submit A (Good) Bug Report?

After you've determined which repository your bug is related to and that the
issue is still present in the latest version of the master branch, create an
issue on that repository and provide the following information:

- Use a **clear and descriptive title** for the issue to identify the problem.
- Explain which **behavior you expected** to see instead and why.
- Describe the exact **steps to reproduce the problem** in as many details as
  necessary.
- When providing code samples, please use [code blocks][code-blocks].

### How Do I Submit A (Good) Enhancement Suggestion?

Instructions are similar to those for bug reports. Please provide the following
information:

- Use a **clear and descriptive title** for the issue to identify the
  suggestion.
- Provide a **description of the suggested enhancement** in as many details as
  necessary.
- When providing code samples, please use [code blocks][code-blocks].

[code-blocks]:
https://help.github.com/articles/creating-and-highlighting-code-blocks/

## Submitting Pull Requests

Instructions are similar to those for bug reports. Please provide the following
information:

- If this is not a trivial fix, consider **creating an issue to discuss first**
  and **later link to it from the PR**.
- Use a **clear and descriptive title** for the pull request.
    - Follow [Conventional Commit
      specification](https://www.conventionalcommits.org/en/v1.0.0/) where
      sufficiently large or impactful change is made.
- Provide a **description of the changes** in as many details as necessary.

Before submitting your pull request, also make sure that the following
conditions are met:

- Your new code **adheres to the code style** through running `cargo fmt`.
- Your new code **passes all existing and new tests** through running `cargo
  test`.
