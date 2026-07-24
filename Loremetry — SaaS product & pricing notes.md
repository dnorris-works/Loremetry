# Loremetry — SaaS product & pricing notes

Product: Web app at loremetry.com (no download)
Audience: Authors
Brand: Loremetry (pseudonymous OK; no personal name required)

------

## Positioning

- Primary: Manuscript analysis and revision
- Secondary: In-app editing around findings and suggestions
- Model: Subscription SaaS

Authors sign in at loremetry.com, upload or manage projects in their account, run reports, apply fixes in the app, and export when they want a copy outside the service.

------

## Subscription tiers

| Tier      | Price                  | What they get                   | Who pays for AI      |
| :-------- | :--------------------- | :------------------------------ | :------------------- |
| BYOK      | Lower ($X/mo)          | Full app access                 | Author (own API key) |
| Hosted AI | Higher ($X + delta/mo) | Full app + monthly AI allowance | You (metered)        |

BYOK

- Connect own API key (Anthropic recommended; optional keys for market reports if offered)
- Pay provider directly for usage
- No monthly AI cap from Loremetry

Hosted AI

- No API setup
- Monthly credit / run allowance included
- Usage monitored; friendly cutoff at limit

------

## Metering (hosted tier only)

Meter actions, not tokens, in marketing and UI.

| Action type | Examples                             | Credits      |
| :---------- | :----------------------------------- | :----------- |
| Light       | Suggest fix, sentence rewrite        | 1 credit     |
| Medium      | Single report on one chapter         | N credits    |
| Heavy       | Full analysis, whole-manuscript pass | Many credits |

Example hosted plan (tune with real cost data):
*500 revision credits + 10 full analysis runs per month*

BYOK tier: show estimated $ per run in-app; no cutoff from Loremetry.

Optional market data (KDP): Canopy or similar — BYOK = optional second key; hosted = include or gate by plan.

------

## Usage monitoring & limits

### Track per account, per billing month

- Credits / runs consumed
- Report type breakdown
- Estimated API cost (internal)
- Failed requests

### Cut off “nicely” at the limit (hosted only)

Before limit

- In-app: *“312 of 500 credits this month”*
- Warning ~80%: *“Resets on [date]”*

At limit

- Stop new AI calls only
- Message: *“Monthly AI allowance used. Add your own API key in Settings to continue now, or wait until [date].”*
- View saved reports, edit without AI, export — still allowed

Avoid: silent failures, locking the account, blocking export.

### BYOK tier

- No enforcement from Loremetry
- Optional usage stats as courtesy only

------

## Operator safeguards

- Daily total hosted-AI spend → alert if above $X
- Single account using unusual share of budget → review
- Kill switch on hosted AI proxy if needed
- Hard cap at published tier limit (optional small one-time grace, sparingly)

------

## Trust & data (SaaS)

Be explicit in privacy policy and UI:

- Manuscript and reports are stored in the user’s Loremetry account so the service can run
- Export anytime (chapters, project, reports)
- Define retention (e.g. delete project, delete account)
- No training on user content (state in policy; align provider terms)
- BYOK: AI calls use their key (direct or via proxy — disclose which)
- Hosted: text sent to AI for that run; minimal logging of full manuscript bodies

Contact: role-based email (e.g. [support@loremetry.com](mailto:support@loremetry.com)); legal entity in footer/ToS if needed, not on marketing hero.

------

## Site structure (loremetry.com)

- / — product, features, pricing (BYOK vs Hosted AI)
- /signup · /login — accounts
- /app — application (post-login)
- /privacy · /terms
- Optional: /docs — setup, API keys, what each report needs

Deep Field Press may link here as “tools for authors”; product home is loremetry.com.

------

## Rollout

1. v1: SaaS at loremetry.com, accounts, projects, core reports, BYOK + app subscription
2. v2: Hosted AI tier, credits, metering, graceful cutoff
3. Price hosted tier above average API cost per user; BYOK remains the off-ramp at limit

------

## Sample pricing page copy

> Loremetry runs in your browser at loremetry.com. Upload your manuscript, run analysis, and revise with AI-assisted suggestions.
>
> BYOK — $X/month
> Bring your own AI API key. You pay the provider for usage. Full access to analysis and revision tools.
>
> Hosted AI — $Y/month
> Includes [N] AI credits per month for reports and rewrites. No API key required. When credits are used, add a key or wait for your next cycle. Export your work anytime.

------

## One-line summary

loremetry.com SaaS with two plans: lower BYOK subscription, higher hosted-AI subscription with visible credits, friendly cutoff, and BYOK as the escape hatch.

------

Paste into your doc as-is. If you want a second section on manuscript upload vs. linked-folder (browser) storage, that can be a short addendum.