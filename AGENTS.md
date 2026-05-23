# AGENTS.md

Codex guide for the Lindelion repository.

## Critical Rules

- Work on `main` by default. Do not create, switch to, or continue work on a non-main branch unless the user explicitly instructs you to use one.
- Never run destructive git commands such as `git reset --hard`, `git checkout --`, or force-push without explicit user approval.
- Never commit secrets, `.env` files, credentials, DAW license files, or private SDK payloads.
- Run `make ci` before committing unless the user explicitly asks for a checkpoint commit.
- Keep the realtime DSP path allocation-free. New audio-thread behavior needs focused no-allocation tests.

## Repository Guide

Read [CLAUDE.md](CLAUDE.md) for the full repository map, commands, product names, and shared architecture rules.
