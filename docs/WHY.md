# The more an AI can do for you, the more it can do to you

*The reasoning behind Fella. Capability and exposure are one axis, and for your
own data the sane direction is down it. What Fella can't do is the point.*

This is the *why* behind Fella. It covers the reasoning behind the project. The
commitments themselves live in `docs/PRINCIPLES.md`, the things Fella
deliberately won't do are in `docs/NON-GOALS.md`, and `docs/DECISIONS.md` is the
running log of how it all got this way.

## The symmetry

An agent that can read your files can read the folder you didn't mean to point
it at. One that can move money can move it wrong. One that can send an email can
send the wrong one to the wrong person. The permission runs both ways. You
cannot grant a capability that only ever helps, because the thing that makes it
useful when the model is right is the same thing that makes it harmful when the
model is wrong, and the model is sometimes wrong in ways that look exactly like
being right.

This isn't cynicism about AI. It's arithmetic about access. Every power you hand
an agent is a power it has now, whether it should use it or not, and whether
you're watching or not.

## Which way the frontier goes

More capability demos better. It raises more money. It makes the video where one
prompt opens a design tool, drives a browser, fills a form, and clears a
checkout. OpenAI's Astra "Work" (September 2026) does exactly that, reaching into
your local files and desktop apps once you grant it access. That is real
engineering, and it is also a direction: up the axis, toward more reach.

Restraint doesn't demo. "It can only do this one thing" is not a launch. So the
effort and the money flow toward what else it can do, and the use that most
needs the restrained version stays unbuilt: your own life's data, where a wrong
answer costs you something real and you are the least equipped to catch it.

The thing I'm arguing against isn't any one model or company. Fella will call
Astra like any other model. It's the assumption underneath the whole race, that
more capability is always the improvement.

## Capability is borrowed trust

When an agent can act, add up what you are trusting. The model, to not invent
the action. The harness, to have no bug on the path that does something you
can't undo. The confirmation dialog, to have asked the right question, at the
right moment, in words you understood. The provider, to not quietly change what
"allowed" means in the next release. Each capability adds a link to that chain,
and a chain breaks at its weakest link on its worst day.

A tool that can only read has almost no chain. You are not trusting it to not
delete your files. It can't.

## The harness matters more than the model

The *harness* is everything around the model: what it may see, which tools it
can call, how its output is checked. It is also where the power ceiling is set.
On the ARC Prize reasoning test the same frontier model scores around 63% bare
and near 99% inside a full harness, thirty-odd points that came from the
wrapper, not the weights.

So the model is the part you swap, not the thing you build around. Fella will
run whatever model you point it at, local or frontier. The design work is in the
harness, and Fella's is *powerfully tiny* on purpose: one linear loop, a small
fixed tool set, no ability to write or reach or act. Small enough that one
person can read all of it and see the ceiling for themselves.

## What that looks like in Fella

Each of these is a power removed on purpose. The job, answering questions about
your folder, survives every cut.

### It can't state a figure

Every number in an answer is a computation the model asked for and a database
produced. A separate pass, run by code and not by the model, re-executes the
cited queries and flags any figure in the answer that traces back to nothing.
The model can reason about your data all it likes. It cannot put a number into a
decision you are about to make unless that number is real. If the folder can't
answer the question, Fella says so and leaves it there.

### It can't look past the folder

You point Fella at one folder, and that folder is the whole world it can see.
That one line does three jobs at once: it's the data, it's the boundary, and
it's a mental model the person already has ("everything in this folder").
Nothing outside it is read. Nothing inside it is written, moved, or deleted.
There is no permission dialog because there is nothing that needs permitting.

### It can't reach the network on its own

The base makes one outbound call, to the model you chose, and by default that
model runs on your machine. Your questions about your money and your health do
not leave the room. If you want Fella to reach a notes service or a wiki, you
connect that yourself, one vetted connector at a time, and it does exactly what
its label says.

### It can't grow without your hand on it

No capability arrives on its own. Extensions are themes, skills, and connectors,
installed one at a time by you, each a known quantity. The base ships closed.
You decide what to open, and nothing opens quietly.

### It's built for someone who won't read a stack trace

The person using Fella isn't a developer. The whole surface is plain language,
it copes with a messy real folder instead of expecting a clean dataset, and
there is no wizard and no settings screen. Setting it up means dropping a
`fella.md` in the folder to explain your own terms. There is no code for you to
write.

### It's small enough to check

The core loop is a few hundred lines. A new dependency has to argue its way in.
A framework can't promise you a power ceiling you can verify by reading. A
product with a fixed scope can, and that is the trade.

## Restraint only counts if it's structural

You cannot add minimal power to a maximal agent. A safe mode bolted onto a tool
that can also act is a setting, and settings get toggled, bypassed, or shipped
wrong. Fella's restraint holds because there is no write tool to disable, none
was built, and no permission prompt to misconfigure, nothing needs one. That is
what makes Fella a different tool, not a careful configuration of the same one.

## What Fella isn't

Fella won't be a coding assistant, a general task agent that files your email
and books your travel, a platform with a plugin runtime, or an account you log
into. The scope is deliberate, and the full "no" list is in `docs/NON-GOALS.md`,
each one with its reason.

## Why the frontier can't follow

OpenAI, Anthropic, and Google run their economics on inference happening on
servers they own. That's the balance sheet, not a preference they're free to
reconsider. Every query gets metered, logged for safety and improvement, and
increasingly folded back into training the next model. A lab whose margin and
whose moat both depend on centralizing your data is never going to lead with
keep it on your own machine, we'll get nothing from you. They can eventually
bolt on a local mode as an enterprise checkbox, years late and buried three
menus deep, the way a big company always ships the feature the smaller one
made popular first. They can't make it the product, because the product is the
data flywheel.

That's the innovator's dilemma, sharper than usual. Normally an incumbent
could cannibalize itself if it had the will. Here it structurally can't
without walking away from the thing its valuation is priced on. The biggest
labs can see this door. Walking through it means giving up the thing they're
funded on, so they won't, and that's exactly the gap Fella sits in.

## The mainframe, again

Computing already ran this arc once. Mainframes were centralized,
institutional, rented by the hour. Then came the personal computer, on the
insurgent bet that a regular person could own the whole stack with no
institution mediating. IBM didn't lead that shift. Mainframes were IBM's
entire business, so the company sitting at the center of gravity was never
going to be the one that dismantled it.

AI is in its mainframe phase right now. Every meaningful model lives in
someone else's datacenter, and nearly every dollar of funding is going toward
a bigger, more centralized version of that. The personal-computer move, a
model good enough to do real work on your own machine, only became technically
possible in the last year or so, and almost nobody funded at scale is building
for it, because scale funding wants the metered, recurring version.

## Trust doesn't creep, it snaps

Privacy sentiment moves in snaps, not a slow creep. Nobody cared much about
mass surveillance until Snowden. Nobody thought twice about how Facebook used
their behavior until Cambridge Analytica. Both practices existed for years
before the reaction arrived all at once.

People are already handing AI companies things they never handed Facebook:
bank statements, health questions, the actual texture of a private life, not
just clicks. That category of data hasn't had its Snowden moment yet. When it
does, whoever already exists as the thing that was never on anyone else's
servers to begin with gets a decade of trust overnight, and everyone else
spends that decade explaining themselves.

## The bet

The big models are a bet that people want one agent that can do everything. I'm
betting that for your own life's data, people want the opposite: an agent that
can do one thing and be trusted with it completely. Both can pay off. They are
not the same product, and they can't be.

## Personal software, reclaimed

Forty years ago "personal computer" was a genuinely radical phrase. It meant
the machine was yours: it worked for you, it kept your secrets, and there was
no institution standing between you and it. Somewhere in fifteen years of
renting everything, that idea got quietly retired. Every piece of software
became a subscription, every subscription became someone else's server, and
"personal" shrank down to meaning your login. AI had the chance to bring that
word back to life and instead it built the most centralized software category
there's ever been, an intelligence that knows you more intimately than
anything before it and answers to someone else's shareholders.

I want Fella to put "personal" back in front of software and mean it the old
way: not a login, something that's actually yours. That's the whole project,
underneath the folder and the SQL and the read-only boundary.

## The gist

- Capability and exposure are one axis. Every power an agent has is a power it
  can misuse.
- The frontier climbs that axis because reach demos well. Your own data needs
  the opposite.
- An agent that can act makes you trust a longer chain: the model, the harness,
  the dialog, the provider.
- The harness is where the power ceiling is set. The model is the part you swap.
- Fella removes powers on purpose: read-only, one folder, never states a figure,
  local by default.
- The job survives every cut. Answering questions about your folder needs none
  of what was removed.
- Restraint only counts if it's structural. A safe setting is not the same as a
  tool that cannot.
- The biggest labs can't follow here without giving up the thing their
  valuation is priced on. Centralizing your data is their revenue, not a
  preference.
- AI is repeating computing's mainframe-to-PC arc, and almost nobody funded at
  scale is building the personal-computer version.
- Privacy sentiment doesn't creep, it snaps. The data people now hand AI is
  more intimate than anything social platforms ever got, and that reckoning
  hasn't happened yet.
- Personal software used to mean something you owned. Fella is a bet on
  bringing that back.
