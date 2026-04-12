# string

## words-collect-loop
**Signature:** `(parts : List) (i : i64) (len : i64) (acc : List) -> List`

---

## words
**Signature:** `(s : String) -> List`
**Intent:** Split a string into words by whitespace

---

## unwords
**Signature:** `(ws : List) -> String`
**Intent:** Join a list of strings with spaces

---

## lines
**Signature:** `(s : String) -> List`
**Intent:** Split a string into lines

---

## unlines
**Signature:** `(ls : List) -> String`
**Intent:** Join a list of strings with newlines

---

## repeat-str
**Signature:** `(s : String) (n : i64) -> String`
**Intent:** Repeat a string n times using binary doubling — O(N log N), O(log N) stack depth

---

## pad-left
**Signature:** `(s : String) (width : i64) (ch : String) -> String`
**Intent:** Pad string to width by prepending ch characters — O(N) via repeat-str

---

## pad-right
**Signature:** `(s : String) (width : i64) (ch : String) -> String`
**Intent:** Pad string to width by appending ch characters — O(N) via repeat-str

---

## is-empty-str
**Signature:** `(s : String) -> bool`
**Intent:** Check if a string is empty

---

## reverse-str
**Signature:** `(s : String) -> String`
**Intent:** Reverse a string

---

## count-occurrences
**Signature:** `(s : String) (sub : String) -> i64`
**Intent:** Count non-overlapping occurrences of sub in s

---

## bytes-match-at
**Signature:** `(haystack : Bytes) (needle : Bytes) (offset : i64) (nlen : i64) (i : i64) -> bool`
**Intent:** Check if needle bytes match haystack at given offset starting from index i

---

## starts-with
**Signature:** `(s : String) (prefix : String) -> bool`
**Intent:** Check if s starts with prefix using byte comparison

---

## ends-with
**Signature:** `(s : String) (suffix : String) -> bool`
**Intent:** Check if s ends with suffix using byte comparison

---

## bytes-scan
**Signature:** `(haystack : Bytes) (needle : Bytes) (hlen : i64) (nlen : i64) (pos : i64) (limit : i64) -> i64`
**Intent:** Find first occurrence of needle in haystack, return byte offset or -1

---

## index-of
**Signature:** `(s : String) (sub : String) -> i64`
**Intent:** Find byte index of first occurrence of sub in s, or -1

---

## contains
**Signature:** `(s : String) (sub : String) -> bool`
**Intent:** Check if s contains sub using byte scanning

---

## to-lower-byte
**Signature:** `(b : i64) -> i64`
**Intent:** Convert ASCII uppercase byte to lowercase

---

## to-upper-byte
**Signature:** `(b : i64) -> i64`
**Intent:** Convert ASCII lowercase byte to uppercase

---

## lower-bytes-loop
**Signature:** `(src : Bytes) (i : i64) (len : i64) -> List`
**Intent:** Collect lowercased bytes as list of 1-byte arrays

---

## upper-bytes-loop
**Signature:** `(src : Bytes) (i : i64) (len : i64) -> List`
**Intent:** Collect uppercased bytes as list of 1-byte arrays

---

## to-lower
**Signature:** `(s : String) -> String`
**Intent:** Convert ASCII uppercase characters to lowercase

---

## to-upper
**Signature:** `(s : String) -> String`
**Intent:** Convert ASCII lowercase characters to uppercase

---

## is-whitespace-byte
**Signature:** `(b : i64) -> bool`
**Intent:** Check if byte is ASCII whitespace (space, tab, newline, carriage return)

---

## trim-left-pos
**Signature:** `(buf : Bytes) (i : i64) (len : i64) -> i64`
**Intent:** Find first non-whitespace byte position from left

---

## trim-right-pos
**Signature:** `(buf : Bytes) (i : i64) -> i64`
**Intent:** Find last non-whitespace byte position from right (exclusive)

---

## trim
**Signature:** `(s : String) -> String`
**Intent:** Trim leading and trailing ASCII whitespace

---

## char-alpha
**Signature:** `(s : String) -> bool`
**Intent:** Check if first character is ASCII alphabetic (A-Z or a-z)

---

## char-digit
**Signature:** `(s : String) -> bool`
**Intent:** Check if first character is ASCII digit (0-9)

---

## char-whitespace
**Signature:** `(s : String) -> bool`
**Intent:** Check if first character is ASCII whitespace

---

