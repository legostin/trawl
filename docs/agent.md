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

## What it can change

The agent is granted Trawl's MCP server as a whole, so it can edit rules,
breakpoints and projects, and use tools your plugins contribute. Ask it to
disable a rule and it will.

**These changes happen immediately, without a confirmation prompt.** That is a
deliberate trade for now, and the reason it is tolerable is the blast radius:
everything the agent can touch is Trawl's own state, visible in the UI and
undoable there. Confirmation cards, showing each change before it happens, are
the next piece of work.

## What it cannot do

It has no access to your files and no shell. Nothing outside the Trawl MCP
server is on its allowlist, and in this mode there is no way for it to ask for
more — so a request that genuinely needs a file or a command is refused rather
than half-attempted.

Support for `codex` and access to your project's repository come later.

## The conversation

One conversation follows you across screens; that is deliberate, so you can
study the traffic on one screen and act on what you found from another without
repeating yourself. **New** clears it and starts a fresh session. **Stop** ends
the turn in flight.
