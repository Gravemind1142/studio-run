# Testing against Open Cloud

The cloud tests run scripts as Open Cloud Luau Execution tasks
([studio-run-cloud.md](../studio-run-cloud.md)). They need a Roblox experience
and an API key of your own. Setup takes about five minutes and nothing about it
is committed: the tests read three settings from a gitignored `.env` file.

These tests are `#[ignore]`d like the Studio ones, so `cargo test` never needs
any of this.

> **Status:** the cloud host is still a spec. The first tests are the probes for
> its open questions (§14 and §15 of the spec), in `tests/cloud_probes/` and
> `tests/cloud_behavior.rs`, with findings recorded in `docs/cloud-behavior.md`.

## 1. Create a test experience

Use a new experience that exists only for these tests. Tasks use the
experience's **real** DataStores, MemoryStores and so on, so never point the
tests at a game you care about.

1. In Studio, open a new **Baseplate**.
2. **File > Publish to Roblox**, as a new experience. Any name will do; keep it
   private.
3. Note its **universe ID** (the experience ID) and **place ID**, from the
   experience's page on the
   [Creator Dashboard](https://create.roblox.com/dashboard/creations).

What's in the place doesn't matter: the tests upload the places they need from
`tests/fixtures/` as new unpublished versions, and run on those.

## 2. Create an API key

On the [Creator Dashboard](https://create.roblox.com/dashboard/credentials),
**Create API Key**:

| Setting | Value |
|---|---|
| Access permissions | **Luau execution sessions**: read and write (`universe.place.luau-execution-session:read`/`:write`). **Places**: write (`universe-places:write`), for uploading test versions. Restrict both to your test experience. |
| Accepted IP addresses | Your public IP. `0.0.0.0/0` works from anywhere, including CI, but then the key works for anyone who has it. |
| Expiration | Short, for example 30 days. |

The dashboard's labels may differ slightly from these; pick the closest match.
The first test to run reports any permission that's missing.

## 3. Fill in `.env`

Copy `.env.example` to `.env` at the repo root and fill it in:

```dotenv
STUDIO_RUN_TEST_UNIVERSE_ID=1234567890
STUDIO_RUN_TEST_PLACE_ID=9876543210
STUDIO_RUN_TEST_API_KEY=<the key>
```

`.env` is in `.gitignore`, so it can't be committed by accident. The tests load
it themselves; there's nothing to run first.

Each setting can also be an environment variable, which wins over the file.
CI uses that. The names are separate from `ROBLOX_API_KEY`, which studio-run
itself reads, so the tests never pick up a key you use for something else.

## 4. Run the tests

```powershell
cargo test --test cloud_behavior -- --ignored --nocapture --test-threads=1
```

A test that can't find a setting fails with a message naming it and pointing
here. The key is never printed, even in a failed test's output.

**Rate limits are shared.** Roblox allows 5 task creations per minute per key
owner, across all their keys, so the tests run one at a time and pace
themselves. Expect a few minutes. Running other tools against Open Cloud from
the same account at the same time slows them down.

## CI

CI runs the cloud tests with the maintainers' experience and key, stored as
repository secrets with the same three names. It runs them only on pushes to
`main` and on manual runs, never on pull requests: GitHub doesn't give secrets
to pull requests from forks, and a pull request's code shouldn't get the key
anyway. If your change affects the cloud host, run the tests with your own
setup and say in the pull request that they passed.
