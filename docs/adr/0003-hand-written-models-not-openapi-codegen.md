---
status: accepted
---

# Hand-written API models that keep unknown fields, not code generated from the OpenAPI document

The control-plane REST client is a published library, so it needs typed request and response models, and the obvious source is Tailscale's OpenAPI document. That document drifts from the live API in known places (a renamed path, enum members the Go client knows and the document does not, two token endpoints it omits), so generated code would need patching after every refresh and would reject fields the API adds. We write the models by hand, one struct per schema, each carrying a flattened map of any unknown fields, and every client method returns the parsed model together with the raw body and the headers that matter. A test parses the vendored OpenAPI document and fails when a schema property has no field on its struct, so refreshing the document is how upstream changes surface.

## Considered options
- **Generate from the OpenAPI document.** Zero typing, but every refresh needs the same patches re-applied, and strict generated types break on additive changes.
- **Untyped JSON values throughout.** What rtailscale does; nothing for library users to compile against, and the server would parse the same JSON twice.
- **Hand-written models with unknown-field retention and a drift test.** Chosen.

## Consequences
- The server forwards the raw body to the model unchanged and tests assert on the typed fields, so both contracts hold at once.
- Because the models tolerate additions, tools declare no output schema for now; enums are strict only for closed sets and free strings elsewhere.
- The drift test is the maintenance loop: refresh the document, run the test, add the fields it names.
