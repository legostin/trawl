# The agent

Trawl can hand your captured traffic to a coding agent and let you talk to it
from any screen. Open the column with the ✦ button in the top bar.

## What runs

Trawl does not embed a model and never asks for an API key. It starts the
harness you already have installed — currently [Claude
Code](https://claude.com/claude-code) — as a child process, so the work runs on
your own subscription, exactly as it would in a terminal.

If `claude` is not on your `PATH`, the panel says so instead of failing
quietly. Install it, then send the message again; no restart is needed.

## What it can see

The agent reads Trawl through Trawl's own MCP server: captured flows and their
bodies, rules, breakpoints, projects. It is given pointers, not dumps — each
message carries a short block naming the screen you are on and what is
selected, and the agent fetches the rest itself. That keeps large response
bodies out of the conversation until they are actually wanted.

If the MCP server is switched off in Settings, Trawl starts it for the session
rather than handing the agent a dead address. Your saved setting is not
changed. When the harness comes up without those tools anyway, the panel says
so — an agent that cannot see the traffic otherwise sounds much like one that
is simply being brief.

## What it cannot do yet

This first version is **read-only**. The agent is launched with an allowlist
holding only the tools that change nothing, so it can explain, count and
compare, but it cannot save a rule, resolve a breakpoint or send a request.
Ask it to change something and it will tell you it cannot.

Tools contributed by plugins are not offered to the agent either. The
allowlist is built from Trawl's own tool registry, which knows which of its
tools change something; a plugin's tool makes no such promise, so it stays out
until there is a way to ask you first.

Writing — with each change confirmed in the app before it happens — is the
next step, along with support for `codex` and access to your project's
repository.

## The conversation

One conversation follows you across screens; that is deliberate, so you can
study the traffic on one screen and act on what you found from another without
repeating yourself. **New** clears it and starts a fresh session. **Stop** ends
the turn in flight.
