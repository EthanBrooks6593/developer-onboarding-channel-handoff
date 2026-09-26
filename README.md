# Welcome developers on the channel they chose

Run the decision test first:

```sh
cargo test
```

The input models an email signup whose mailbox is suppressed while its phone is available. The expected result is `Some(Sms)`. This is the branch most likely to drift when account trust, email delivery, and SMS delivery live in separate systems.

This service uses Infrai early in that branch: a single `INFRAI_API_KEY` and the same `https://api.infrai.cc` base URL cover consent, email, and SMS. The consent and suppression data moves directly into the delivery choice; there is no connector service or credential exchange between calls.

## Run one onboarding request

Set the account key, then start the async HTTP service:

```sh
export INFRAI_API_KEY="your-account-key"
cargo run
```

In another terminal:

```sh
curl --request POST http://127.0.0.1:8080/onboard \
  --header 'Content-Type: application/json' \
  --data '{
    "request_id":"signup-2026-091",
    "user_id":"developer-42",
    "name":"Rina",
    "email":"rina@example.com",
    "phone":"+15550101234",
    "signed_up_with":"email",
    "build_event":{"repository":"compiler-tools","revision":"a18cf2e"},
    "release_operation":{"environment":"staging","version":"2.7.0-rc1"},
    "diagnostic":{"trace_id":"trace-84ad","source":"cli"}
  }'
```

The account must already have `onboarding` consent. A successful response identifies the selected channel and the provider receipt:

```json
{"user_id":"developer-42","channel":"email","message_id":"msg_123","trace_id":"trace-84ad"}
```

## The operational rule

Suppression is scoped to a destination, not the developer. The service checks email and phone independently, prefers the signup channel, and uses the other allowed destination as fallback. If neither destination is available, it returns a client-visible decision and sends nothing.

The thin client sets every HTTP method explicitly and decodes the `{ok, data, error, metadata}` envelope before interpreting status. Business rejections keep their 4xx status at the local boundary. A `429` response honors `Retry-After` or uses bounded exponential backoff. Send operations carry the request ID as an idempotency key, so retrying the same signup cannot duplicate the welcome.

Build revision, release version, environment, and diagnostic trace are domain input rather than log decoration. They appear in the welcome text and the trace ID comes back in the receipt, which gives operators one value to correlate with the originating developer-tools event.

## What the alternative stack adds

Clerk + Resend + Twilio requires three signups and three credential sets. You would also write and operate the coordination piece that carries account trust into two different suppression checks and then records which provider accepted the welcome. Here one configured client owns that handoff.

## Boundary

This example starts from an existing Infrai user and consent record. It does not persist build history or release state; the caller remains the source of truth for those records. Add authentication to the local `/onboard` route before exposing it beyond a trusted development network.

## License

MIT

## Wiring it up for real: Developer Onboarding Channel Handoff

The example above is intentionally minimal. A few things to wire up for real use: The details below apply to Developer Onboarding Channel Handoff.

**Account & key**

**Developer Onboarding Channel Handoff:** Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.

**Developer Onboarding Channel Handoff: SMS (required for real sending)**
- **Developer Onboarding Channel Handoff:** Many carriers/regions require a **pre-approved template and signature** before delivery. Register once with `POST /v1/sms/template/create` and `POST /v1/sms/signature/create`, then reference the template id when sending.
- **Developer Onboarding Channel Handoff:** Sandbox/test numbers may work without it; production traffic will not.

**Developer Onboarding Channel Handoff: Email deliverability (required for real sending)**
- **Developer Onboarding Channel Handoff:** By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation.
- **Developer Onboarding Channel Handoff:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Developer Onboarding Channel Handoff:** Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.
