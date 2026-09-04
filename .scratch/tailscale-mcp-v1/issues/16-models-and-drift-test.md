# 16 — Typed models and the schema drift test

Status: ready-for-agent
Milestone: 3 — Tailnet surface
Blocked by: 15

Hand-written models for the API's schemas, each retaining unknown fields, per ADR-0003. Every client method returns the parsed model together with the raw body and the headers that matter, so the server can forward raw output while tests assert on typed fields.

Enums are strict only for genuinely closed sets; the sets known to drift are documented strings. The drift test parses the vendored API description and fails when a schema property has no corresponding field.

## Acceptance criteria
- A response containing an unknown field parses, and the field is retrievable.
- The drift test passes against the vendored description and fails when a property is removed from a model.
- The known divergences between the description and the live API are recorded where the test can explain them rather than silently pass.
- Documented strings carry their known values in the parameter description.
