# How to release a zellij version

This document is primarily target at zellij maintainers in need to (prepare to)
release a new zellij version.


## Simulating a release

This section explains how to do a "dry-run" of the release process. This is
useful to check if a release is successful beforehand, i.e. before publishing
it to the world. Because there is no "undo"-button for a real release as
described below, it is recommended to perform a simulated release first.


### Requirements

You only need a publicly accessible Git repository to provide a cargo registry.


### High-level concept

The setup explained below will host a third-party cargo registry software
([ktra](https://github.com/moriturus/ktra)) locally on your PC. In order for
`cargo` to pick this up and be able to work with it, we must perform a few
modifications to the zellij repository and other components. Once setup, we
release a zellij version to this private registry and install zellij from there
to make sure it works as expected.


### Step-by-step guide

1. Create a cargo index repository
    1. Create a new repo on some git forge (GitHub/GitLab/...)
    1. Clone the repo **with HTTPS (not SSH)**, we'll refer to the `https://`
       clone-url as `$INDEX_REPO` for the remainder of this text
    1. Add a file named `config.json` with the following content in the root:
       ```json
       {"dl":"http://localhost:8000/dl","api":"http://localhost:8000"}
       ```
    1. Generate an access token for full repo access, we'll refer to this as
       `$TOKEN` for the remained of this text
    1. Create and push a commit with these changes. Provide the following HTTPS
       credentials:
        1. Username: Your git-forge username
        1. Password: `$TOKEN`
1. Prepare the zellij repo
    1. `cd` into your local copy of the zellij repository
    1. Add a new cargo registry to `.cargo/config.toml` like this:
       ```toml
       
       [registries]
       ktra = { index = "https://$INDEX_REPO" }
       ```
    1. Modify **all** `Cargo.toml` in the zellij repo to retrieve the individual
       zellij subcrates from the private registry:
        1. Find all dependencies that look like this:
           ```toml
           zellij-utils = { path = "../zellij-utils/", version = "XXX" }
           ```
        1. Change them to look like this
           ```toml
           zellij-utils = { path = "../zellij-utils/", version = "XXX", registry = "ktra" }
           ```
        1. This applies to all zellij subcrates, e.g. `zellij-client`,
           `zellij-server`, ... You can ignore the plugins, because these aren't
           released as sources.
1. Launch your private registry
    1. Create the file `~/.cargo/config.toml` with the following content:
       ```
       [registries.ktra]
       index = "https://$INDEX_REPO"
       ```
    1. Install `ktra`, the registry server: `cargo install ktra`
    1. In a separate shell/pane/whatever, navigate to some folder where you
       want to store all data for the registry
    1. Create a config file for `ktra` named `ktra.toml` there with the
       following content:
       ```toml
       [index_config]
       remote_url = "https://$INDEX_REPO"
       https_username = "your-git-username"
       https_password = "$TOKEN" 
       branch = "main"  # Or whatever branch name you used
       ```
    1. Launch ktra (with logging to see what happens): `RUST_LOG=debug ktra`
    1. Get a registry token for `ktra` (The details don't really matter, unless
       you want to reuse this registry):
       ```bash
       curl -X POST -H 'Content-Type: application/json' -d '{"password":"PASSWORD"}' http://localhost:8000/ktra/api/v1/new_user/ALICE
       ```
    1. Login to the registry with the token you received as reply to the
       previous command:
       ```bash
       cargo login --registry ktra "KTRA_TOKEN"
       ```
1. **Install safety measures to prevent accidentally performing a real release**:
    1. In your `zellij` repo, remove all configured remotes that allow you to
       push/publish directly to the zellij main GitHub repo. Setup a fork of
       the main zellij repo instead and configure a remote that allows you to
       push/publish to that. Please, this is very important.
    1. Comment out the entire `[registry]` section in `~/.cargo/credentials` to
       prevent accidentally pushing a new release to `crates.io`.
1. **Simulate a release**
    1. Go back to the zellij repo, type:
       ```bash
       cargo x publish --git-remote <YOUR_ZELLIJ_FORK> --cargo-registry ktra
       ```
    1. A prompt will open with the commit message for the release commit. Just
       save and close your editor to continue
    1. If all goes well, the release will be done in a few minutes and all the
       crates are published to the private `ktra` registry!
1. Testing the release binary
    1. Install zellij from the registry to some local directory like this:
       ```bash
       $ cargo install --registry ktra --root /tmp zellij
       ```
    1. Execute the binary to see if all went well:
       ```bash
       $ /tmp/bin/zellij
       ```
1. Cleaning up
    1. Uncomment the `[registry]` section in `~/.cargo/config.toml`
    1. Restore your original git remotes for the zellij repo
    1. Undo your last commit:
       ```bash
       $ git reset --hard HEAD~1
       ```
    1. Undo your last commit in the remote zellij repo:
       ```bash
       $ git push --force <YOUR_ZELLIJ_FORK>
       ```
    1. Delete the release tag:
       ```bash
       $ git tag -d "vX.Y.Z"
       ```
    1. Delete the release tag in the remote zellij repo
       ```bash
       $ git push <YOUR_ZELLIJ_FORK> --force --delete "vX.Y.Z"
       ```

You're done! :tada:


## Releasing a new version
