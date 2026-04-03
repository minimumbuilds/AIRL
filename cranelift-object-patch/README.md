# cranelift-object-patch

Vendored copy of `cranelift-object` 0.130.0 with the `object` dependency
upgraded from 0.36 to 0.38.1.

## Why

The upstream `cranelift-object` 0.130.0 pins `object = "0.36"`, which conflicts
with `airl-runtime`'s need for `object = "0.38"` features. This patch bumps the
`object` dep to 0.38.1 so the workspace can unify on a single `object` version.

## When to remove

Once upstream `cranelift-object` ships with `object >= 0.38` (expected in the
next Cranelift release cycle), this vendored copy can be replaced with the
upstream crate and this directory deleted.
