# AI Usage Policy

TL;DR: AI-designed code is technical debt, and must never make it to production. No AI for human-facing documentation.

Tenets:

 1. *0% AI in human-facing documentation.* If you want someone to understand something, take the time to understand it enough to write an article on it. Also, AI is terrible at writing.
 2. *Don't let AI make decisions.* AI is fundamentally trained to spend the least amount of effort until unit tests are green. In other words, it hacks. It wastes/spends your hard-won design/stability/zen, and accrues tech debt.
 3. Because of #2, *Don't let AI program.* Programming is full of decisions, and AI usually makes the wrong decisions.
 4. We distinguish _programming_ from _changing code_ however. Using AI for mechanical refactors and mechanical small changes are fine, because the AI is not making decisions.
 5. Exception to #4: Verdagon will *temporarily* allow AI-designed code if it's in *isolated submodules* that are *pure* (don't modify inputs) and only *use pure APIs.* However, know this is *technical debt,* and should be *tracked*, *isolated*, have its *removal planned+designed*, and be _actually removed_ before the next (minor) release.


## AI makes technical debt

The biggest realization from all this is that *AI-designed code is technical debt.*

Any experienced software engineer recognizes this when they _actually look_ at the code that Claude generates.

If anyone is unconvinced of this, I recommend they do an *experiment:* make requirements for, then design, then implement a *medium-sized feature*. Then, give Claude just the requirements. Compare the outputs.

In my experience, Claude's implementation is worse ~80% of the time, equal ~18%, and more clever ~2% (note I say "more clever" instead of better).


## When to use AI

All that said, technical debt can be wielded well, if someone has the discipline. If you show that you can use it with discipline and care, your AI-generated code will be allowed, at least in certain isolated areas of the codebase.

Talk to Verdagon _before_ you submit code where any part of it is AI-generated. Any PRs submitted with AI-generated code without talking to Verdagon first will be closed.
