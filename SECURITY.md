# Security Policy

We take the security of compas seriously and appreciate responsible disclosure.

## Supported versions

compas is in active development. Security fixes target the **latest release** and the current
`main` branch. Older versions are not maintained — please update before reporting an issue you hit
on an outdated build.

## Reporting a vulnerability

**Please do not open a public issue for security problems.** Report them privately so a fix can ship
before details are public:

- Use the repository's **private vulnerability reporting** (the "Report a vulnerability" option
  under the project's Security tab), or
- Contact a maintainer directly through a private channel.

Include as much as you can:

- A clear description of the issue and its impact.
- Steps to reproduce, or a proof of concept.
- Affected version, platform, and configuration.
- Any suggested mitigation, if you have one.

## What to expect

- **Acknowledgement** of your report within a few days.
- An assessment of severity and scope, and updates as we investigate.
- A coordinated fix and release, with credit to the reporter if desired.

## Scope notes

compas runs locally and does true DSP only on local, DRM-free files. Areas of particular interest:
the auto-update path (signed payloads and update integrity), file and library handling (untrusted
audio files and database state), any future networked or streaming features, and the desktop shell
boundary. Reports about third-party dependencies are welcome; where possible we forward them
upstream and track the fix.

Thank you for helping keep compas and its users safe.
