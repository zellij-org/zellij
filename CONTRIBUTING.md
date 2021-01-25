# Contributing to Mosaic

Thanks for considering to contribute to mosaic!

Before contributing please read our [Code of Conduct](CODE_OF_CONDUCT.md) which
all contributors are expected to adhere to.

## Add an entry to the changelog

If your contribution changes the behavior of `mosaic` (as opposed to a typo-fix
in the documentation), please update the [`CHANGELOG.md`](CHANGELOG.md) file
and describe your changes. This makes the release process much easier and
therefore helps to get your changes into a new `mosaic` release faster.

The top of the `CHANGELOG` contains a *"unreleased"* section with a few
subsections (Features, Bugfixes, …). Please add your entry to the subsection
that best describes your change.

Entries follow this format:
```
- Short description of what has been changed, see #123 (@user)
```
Here, `#123` is the number of the original issue and/or your pull request.
Please replace `@user` by your GitHub username.

## Lacking API for plugin in mosaic

If you have a plugin idea, but mosaic still doesn't have API required to make
the plugin consider opening [an issue][plugin-issue] and describing your
requirements.

[plugin-issue]: https://github.com/mosaic-org/mosaic/issues/new?assignees=&labels=plugin%20system
