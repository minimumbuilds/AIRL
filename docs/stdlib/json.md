# json

## json-escape-loop
**Signature:** `(src : Bytes) (i : i64) (len : i64) -> List`
**Intent:** Build list of byte chunks with JSON escapes applied

---

## json-escape
**Signature:** `(s : String) -> String`
**Intent:** Escape special characters in a string for JSON output

---

## json-stringify-list-loop
**Signature:** `(lst : List) (first? : Bool) -> String`
**Intent:** Stringify list elements separated by commas using head/tail recursion

---

## json-stringify-map-loop
**Signature:** `(entries : List) (first? : Bool) -> String`
**Intent:** Stringify map key-value pairs separated by commas using head/tail recursion

---

## json-stringify
**Signature:** `(val : Any) -> String`
**Intent:** Serialize an AIRL value to a JSON string

---

## json-skip-ws
**Signature:** `(src : Bytes) (pos : i64) (len : i64) -> i64`
**Intent:** Skip whitespace (space, tab, newline, CR) starting at pos

---

## hex-digit-to-int
**Signature:** `(b : i64) -> i64`
**Intent:** Convert an ASCII hex digit byte (0-9, a-f, A-F) to its integer value

---

## unicode-escape-to-bytes
**Signature:** `(src : Bytes) (pos : i64) -> Bytes`
**Intent:** Decode 4 hex digits at pos into UTF-8 bytes for the Unicode codepoint

---

## json-parse-string-loop
**Signature:** `(src : Bytes) (pos : i64) (len : i64) -> List`
**Intent:** Collect string bytes handling escapes (including \uXXXX), return [bytesList, endPos]

---

## json-parse-string
**Signature:** `(src : Bytes) (pos : i64) (len : i64) -> List`
**Intent:** Parse JSON string, return [string-value, position-after-closing-quote]

---

## json-parse-number-end
**Signature:** `(src : Bytes) (pos : i64) (len : i64) -> i64`
**Intent:** Find position after last digit/dot/sign/e of number

---

## json-has-dot
**Signature:** `(src : Bytes) (start : i64) (end : i64) -> bool`
**Intent:** Check if byte range contains a dot (for float detection)

---

## json-parse-int-loop
**Signature:** `(src : Bytes) (pos : i64) (end : i64) (acc : i64) (neg : bool) -> _`
**Intent:** Accumulate digits into an integer, returning Err on i64 overflow

---

## json-parse-number
**Signature:** `(src : Bytes) (pos : i64) (len : i64) -> List`
**Intent:** Parse JSON number, return [value, next-position]

---

## json-parse-array-loop
**Signature:** `(src : Bytes) (pos : i64) (len : i64) -> List`
**Intent:** Parse comma-separated array elements, return [elements-list, next-position]

---

## json-parse-object-loop
**Signature:** `(src : Bytes) (pos : i64) (len : i64) (acc : Map) -> List`
**Intent:** Parse comma-separated key:value pairs, return [map, next-position]

---

## json-parse-value
**Signature:** `(src : Bytes) (pos : i64) (len : i64) -> List`
**Intent:** Parse a JSON value at pos, return [value, next-position]

---

## json-parse
**Signature:** `(s : String) -> Any`
**Intent:** Parse a JSON string into an AIRL value

---

