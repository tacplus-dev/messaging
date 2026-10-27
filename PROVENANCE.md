# Provenance

Copied **wholesale** on 2026-10-23 from `~/workspace/gitlab/seabeam/schematic/platform/services/messaging` at
commit `5c63b73` (branch `big8-p1-messaging`), with `proto/services/messaging.proto`, the common/vservices protos it imports,
and `proto/dependencies.yaml` from `platform/proto` at `57f45fe` on branch `big8-p1-messaging`. Build outputs not copied.

Jody, 2026-10-23: the platform's primitives move to tacplus-dev as ships in the night; the private copy runs
the launch fleet unchanged and new-design changes land here.

Divergences: `kind: service`; headers in tacplus.yaml (Jody Scott, Apache-2.0; LICENSE added); `src: proto`;
`proto/registry.yaml` is a lock of github.com/tacplus-dev/registry.

SeaBeam references removed on 2026-10-27 (#3): `pkg` is `com.github.tacplusdev.messaging`, the registry lock
names this repository as the contract, `proto/dependencies.yaml` has no `registry:` block (GitLab project,
SeaBeam crate and package names), the runtime base is `gcr.io/distroless/cc-debian13`, the topic's
`s2s_subscribe` no longer grants the upstream's agent service (`scottyagent`, not a tacplus-dev service), and
comments no longer cite SeaBeam products. The proto is vendored here (`proto/services/messaging.proto`); its
imports come from github.com/tacplus-dev/registry.
