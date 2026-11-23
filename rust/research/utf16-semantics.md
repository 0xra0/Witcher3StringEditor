# UTF-16 / lowercase semantics for the C# -> Rust port

Scope: `LocalizationKeyHash.Of` and `StoredText` (v162 UTF-16LE obfuscation).
Evidence: recorded cases under `rust/app-cases/hash-*`, the two C# sources, and the harness escape rule.

## 1. The hash algorithm (exact)

`LocalizationKeyHash.Of` (`LocalizationKeyHash.cs:31-44`):
1. `lower = key.ToLowerInvariant()`  (`:34`)
2. `utf16 = Encoding.Unicode.GetBytes(lower)` — UTF-16LE bytes (`:35`)
3. `hash = 0`; for `i = 0,2,4,...`: `unit = utf16[i] | (utf16[i+1] << 8)` (little-endian reconstruction == the raw UTF-16 code unit), then `hash = hash*31 + unit` in `unchecked` u32 (`:36-42`).

So the hash is over the **UTF-16 code units of the invariant-lowercased key**, each unit folded in as `hash = hash.wrapping_mul(31).wrapping_add(unit)`. No BOM, no length prefix, wrapping u32 arithmetic. Empty key -> `0x00000000` (`hash-empty.stdout`).

## 2. `ToLowerInvariant` — what the recorded inputs require

`ToLowerInvariant` is **invariant-culture, simple (one-code-point -> one-code-point) lowercase**: it never expands a character. Verified directly against .NET 10 (`dotnet run` probe):

- ASCII `A-Z` -> `a-z`; `LABEL_MAIN_MENU_OPTIONS` -> `label_main_menu_options` (`LocalizationKeyHash.cs:34`).
- CJK `杰洛特` and symbols/emoji `🎮⚔️` have no case mapping -> **unchanged**.
- **U+0130 `İ` (LATIN CAPITAL I WITH DOT ABOVE) maps to ITSELF**, not to `i`, and not to `i`+combining-dot. This is the decisive case.
- Other simple mappings are one-to-one: `ẞ`->`ß`, `ǅ`->`ǆ`, `Σ`->`σ`, `KELVIN SIGN`->`k`, supplementary `U+10400`->`U+10428` (the surrogate pair's low unit changes `DC00`->`DC28`).

Recorded values confirm this (`hash-*.stdout`, inputs from `hash-*.toml`):

| case | key | expected hash |
|---|---|---|
| empty | `""` | `0x00000000` |
| simple | `label_main_menu_new_game` | `0xB28F5136` |
| upper | `LABEL_MAIN_MENU_OPTIONS` | `0x6E50BFF9` |
| cjk | `杰洛特` | `0x0191F42E` |
| emoji | `🎮⚔️` | `0x6598C46D` |
| turkish | `İstanbul` | `0xEA4E763B` |

Discriminator proof (computed `hash*31+unit` over UTF-16 units):
- `İstanbul` with `İ` kept as `U+0130` -> `0xEA4E763B` **MATCH**.
- with `İ` -> `i` (`U+0069`) -> `0x2A4494E2`; with `İ` -> `i`+`U+0307` (Rust `char::to_lowercase`, Python `str.lower`) -> `0xD9A98BED`; with dotless `ı` (`U+0131`) -> `0x522FA31A`. All wrong. So a naive `String::to_lowercase()` / per-char full `to_lowercase()` fails the recorded Turkish case.

**Rust implementation rule.** Do not use `str::to_lowercase()` (it applies full Unicode case folding; `İ` expands to two scalars). Iterate **scalar values** (`char`s), and for each apply only a single-scalar (simple) mapping:

```
for ch in key.chars():
    let mut it = ch.to_lowercase()
    let first = it.next()
    folded_ch = if it.next().is_none() { first } else { ch }  // expansion -> keep original
    push folded_ch
then hash the UTF-16 units of the folded string (str::encode_utf16)
```

Rationale: Rust `char::to_lowercase()` yields exactly one scalar for every simple mapping (`A`->`a`, `ẞ`->`ß`, `Σ`->`σ`, Deseret `U+10400`->`U+10428`) and two scalars only where .NET keeps the character unchanged (`İ`->`i`+`U+0307`). Taking the single-scalar result, else the original, reproduces `ToLowerInvariant` for all six recorded inputs **and** correctly lowers supplementary cased scalars.

Caveat: the existing port (`port.rs:91-109`) applies this rule **per `u16` unit**, so a supplementary cased scalar (e.g. `U+10400`, units `D801 DC00`) is left untouched because `char::from_u32(0xD801)` is `None`; .NET would lower it to `D801 DC28`. No recorded case exercises this, but the correct implementation must fold per scalar, then `encode_utf16()`. Also note .NET invariant casing uses its own Unicode version; exotic characters could diverge from Rust's tables, but all six recorded inputs agree.

## 3. Lone surrogates — representation

A C# `string` is UTF-16 and **can hold unpaired surrogate code units** (`U+D800..U+DFFF`). Relevant behaviours, verified against .NET 10:

- `ToLowerInvariant` leaves a lone surrogate unit unchanged.
- `Encoding.Unicode.GetBytes` (encode) replaces a lone surrogate with the **U+FFFD replacement**: a lone high surrogate `D83C` encodes to bytes `FD FF` (LE for `U+FFFD`). Hashing such a key would fold in unit `0xFFFD`, not the raw `0xD83C`. (Harness keys come from argv and cannot normally carry a lone surrogate, so no recorded hash case does this.)
- `Encoding.Unicode.GetString` (decode) of bytes carrying a lone surrogate unit yields `U+FFFD`; a valid surrogate pair survives as the pair (one scalar in a C# string). This is exactly `StoredText.Decode` for v162 (`StoredText.cs:107-110`, `:196-202`, unit-2 branch calls `Encoding.Unicode.GetString(bytes)`).

**Rust representation:** Rust `String` cannot contain a lone surrogate, and it does not need to. Use `String::from_utf16_lossy(&units)` — it replaces each unpaired surrogate with `U+FFFD`, matching .NET `Encoding.Unicode.GetString` byte-for-byte. The existing port's `decode_utf16` (`port.rs:227`, `String::from_utf16_lossy`) is therefore correct for decoded text. Do not try to smuggle raw surrogate units into a `String`.

If an implementation ever needs to mirror the *encode* fallback (hashing a key that contains a lone surrogate), map each unpaired surrogate to `0xFFFD` before `encode_utf16`; for the recorded inputs this path is never taken.

## 4. v162 obfuscation (context, unchanged by case semantics)

`StoredText.From`/`Decode` (`StoredText.cs:65-70`, `:143-146`, `:165-170`, `:226-245`, `:253-256`): `length = encoded_bytes.len()/unit` (= number of UTF-16 code units for unit 2, surrogate pairs count as 2). First key `k0 = (magic >> 8) & 0xFFFF` (`:213-216`). Per character `i`: `ck = (length+1) * k` (u32), XOR low byte into `byte[2i]`, high byte into `byte[2i+1]`, then `k = rotate_left_16(k, 1)`. Obfuscation is involutive, so encode and decode use the same routine. Case folding never applies here — it operates on raw UTF-16LE bytes.

## 5. Harness JSON escape rule (for dump comparison)

`Program.cs:9-15` and `:295-330`: `"`->`\"`, `\`->`\\`, `\b\f\n\r\t` named, other `<0x20` -> `\u00xx` lowercase; a **lone surrogate unit** -> `\uxxxx` lowercase; a valid pair passes through as UTF-8; everything else UTF-8; no BOM, compact, LF. Note that decode output cannot contain lone surrogates (§3), so the `\uxxxx` surrogate branch is defensive; it does not fire on decoded strings. Noncharacters such as `U+FFFF` are not surrogates and pass through as UTF-8 (`EF BF BF`).
