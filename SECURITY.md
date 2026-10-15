# Security

Wit runs a small web server on your own computer. It can read and change the
documents you open in it, so a flaw in it matters. Thank you for reporting one.

## Reporting a problem

Please don't open a public issue. Use **Report a vulnerability** on the
repository's **Security** tab instead; only the maintainers see it. Say what
you found, how to reproduce it, and which version or commit you used.

You will get a reply within a week. Once a fix is released, you are welcome
to be credited in the release notes.

## What Wit does to stay safe

- The web server listens only on this computer (`127.0.0.1`) unless you start
  it with `WIT_HOST=0.0.0.0`. Do that only on a network you trust: anyone on
  it can then open and change your documents.
- It answers only requests addressed to `localhost` or an IP address, so a
  web page can't reach it through a domain name of its own (DNS rebinding).
- The desktop app runs the same server inside the app, on a random port,
  listening only on this computer.
- Wit needs no account and sends nothing over the internet. Your documents
  stay in the folders you choose.
