# Official packages

Packages published to the Wares registry under `@alliecatowo/`. Each is a copy of the matching
`stdlib/std/` module with a `lumen.toml`; keep them in sync when the stdlib changes and bump the
version before republishing.

| Package | Source |
|---------|--------|
| `@alliecatowo/text` | `stdlib/std/text.lm.md` |
| `@alliecatowo/math` | `stdlib/std/math.lm.md` |
| `@alliecatowo/collections` | `stdlib/std/collections.lm.md` |
| `@alliecatowo/testing` | `stdlib/std/testing.lm.md` |

Publish (needs a GitHub login that is in the registry's `ALLOWED_PUBLISHERS`):

```bash
wares login --provider github
for p in text math collections testing; do (cd packages/$p && wares publish); done
```
