# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Email **opensource@tpt.solutions** with:
- A description of the vulnerability and its impact
- Steps to reproduce or a proof-of-concept
- Any suggested mitigations

You will receive an acknowledgement within 48 hours and a resolution timeline within 7 days.

## Scope

- All crates in the `tpt-async` workspace
- TLS configuration defaults in `tpt-net-tls`
- Memory safety issues in unsafe blocks (all unsafe is documented)
