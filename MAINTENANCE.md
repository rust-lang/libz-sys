# Maintenance Guide

This repository publishes two crates from the same sources:

- `libz-sys`, from [`Cargo.toml`](Cargo.toml)
- `libz-ng-sys`, from [`Cargo-zng.toml`](Cargo-zng.toml)

The supported release entrypoint is the maintenance tool in `maint/`.

## Release Steps

1. Update both crate versions so that `Cargo.toml` and `Cargo-zng.toml` have
   the exact same `package.version`.
2. Make sure the release commit is checked out with submodules initialized.
3. Run the release verification in dry-run mode:

   ```bash
   cargo run -p maint -- publish
   ```

   This verifies all release packaging and build combinations with
   `cargo publish --dry-run`, checks that both manifests have the same version,
   and rejects missing bundled source files or missing packaged source files.

4. After the dry-run passes, tag the release commit with the exact version
   from both manifests, such as `1.2.3`. The tag must point directly at `HEAD`;
   a tag on an earlier commit or a legacy `1.2.3-zng` tag alone is not sufficient.
   Lightweight and annotated tags are both supported.

5. Push the release commit and its version tag to trigger the publish workflow.
   To publish locally instead, run:

   ```bash
   cargo run -p maint -- publish --execute
   ```

   The tool checks the release tag before packaging, verifies the builds, then
   publishes `libz-sys` and `libz-ng-sys`.

## Safety Rules

- `maint publish` defaults to dry-run mode. Real publishing requires
  `--execute` and a tag on `HEAD` matching the version in both manifests.
  Dry-runs do not require a tag, so branch and pull request CI can verify packaging.
- The tool never relies on a bare `cargo publish`. Verification always uses
  `cargo publish --dry-run`, and the upload step uses `cargo publish --no-verify`
  only after all dry-run checks succeed.
- The release path does not use `--allow-dirty`; Cargo should reject a dirty
  worktree.
- Missing submodule checkouts are caught before publishing by requiring bundled
  zlib and zlib-ng source files to exist in the worktree and in the packaged
  crate contents.

## Working on `libz-ng-sys`

Use the maint tool to run Cargo as if the repository were checked out as the
`libz-ng-sys` crate:

```bash
cargo run -p maint -- zng test
```

The compatibility wrapper still works:

```bash
./cargo-zng test
```

If you invoke `publish` through the `zng` command, the tool forces
`--dry-run`. Real publishing is only supported through `maint publish
--execute`.
