# An illumos-based CI

This is just my ad-hoc, fast CI platform.

The base idea is to abuse zones and ZFS to provide very fast and furious experience.

## Exploring the interface

The interface is a bit reminiscent of zoneadm and other illumos interactive cli tools.
teisuu has the server and the cli tool.

```sh
$ teisuu create-pipeline katarineko  # Creates a pipeline with two packages, this will initialize the zone
> addpackage elixir
> addpackage rust
> addrepo https://github.com/MarceColl/katarineko  # repo to clone
> commit

Creating pipeline katarineko

Creating zone at /zones/teisuu/katarineko/base
Installing packages
Cloning repo to /build/katarineko
Running build.sh

Webhook URL: teisuu.gyojuu.org/webhooks/katarineko
Pipeline URL: teisuu.gyojuu.org/pipeline/katarineko
```

When a pipeline is created or updated a base zone is created, the packages are installed and the repo
is cloned. Then the CI script runs. This sets the base filesystem for the pipeline. All future
runs will spawn from this filesystem.

The base zone can be updated by running `teisuu refresh-pipeline katarineko`.
