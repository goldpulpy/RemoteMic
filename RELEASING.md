# Releasing RemoteMic

RemoteMic releases use Semantic Versioning tags in the `vMAJOR.MINOR.PATCH`
format. The tag and the package version in `Cargo.toml` must match.

## Publish a release

1. Update `package.version` in `Cargo.toml` and run `cargo check` so that
   `Cargo.lock` receives the same version.
2. Commit the version change and push it to `main`.
3. Create and push an annotated tag for that commit:

   ```bash
   git tag -a v1.2.3 -m "RemoteMic v1.2.3"
   git push origin v1.2.3
   ```

The release workflow verifies formatting, Clippy, tests, the installer, and the
tag/package version match. It then builds the static Linux x86_64 binary,
publishes it with a SHA-256 checksum, marks the release as latest, and generates
release notes from the merged pull requests since the previous release.

If any verification fails, no GitHub Release is created. Keep published tags
immutable: fix the problem and publish a new patch version instead of moving or
reusing the failed tag.
