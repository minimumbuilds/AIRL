# builtins

Compiler-intrinsic functions always available without import.

## +
**Category:** math
**Signature:** `(a : Int|Float) (b : Int|Float) -> Int|Float`
**Description:** Add two numbers or concatenate two strings.

---

## -
**Category:** math
**Signature:** `(a : Int|Float) (b : Int|Float) -> Int|Float`
**Description:** Subtract b from a.

---

## *
**Category:** math
**Signature:** `(a : Int|Float) (b : Int|Float) -> Int|Float`
**Description:** Multiply two numbers.

---

## /
**Category:** math
**Signature:** `(a : Int|Float) (b : Int|Float) -> Int|Float`
**Description:** Divide a by b. Integer division truncates toward zero.

---

## %
**Category:** math
**Signature:** `(a : Int) (b : Int) -> Int`
**Description:** Integer remainder of a divided by b.

---

## =
**Category:** type
**Signature:** `(a : Any) (b : Any) -> Bool`
**Description:** Structural equality: true if a and b are equal.

---

## !=
**Category:** type
**Signature:** `(a : Any) (b : Any) -> Bool`
**Description:** Structural inequality: true if a and b differ.

---

## <
**Category:** math
**Signature:** `(a : Int|Float) (b : Int|Float) -> Bool`
**Description:** True if a is strictly less than b.

---

## >
**Category:** math
**Signature:** `(a : Int|Float) (b : Int|Float) -> Bool`
**Description:** True if a is strictly greater than b.

---

## <=
**Category:** math
**Signature:** `(a : Int|Float) (b : Int|Float) -> Bool`
**Description:** True if a is less than or equal to b.

---

## >=
**Category:** math
**Signature:** `(a : Int|Float) (b : Int|Float) -> Bool`
**Description:** True if a is greater than or equal to b.

---

## and
**Category:** type
**Signature:** `(a : Bool) (b : Bool) -> Bool`
**Description:** Logical AND of two booleans.

---

## or
**Category:** type
**Signature:** `(a : Bool) (b : Bool) -> Bool`
**Description:** Logical OR of two booleans.

---

## not
**Category:** type
**Signature:** `(a : Bool) -> Bool`
**Description:** Logical NOT of a boolean.

---

## xor
**Category:** type
**Signature:** `(a : Bool) (b : Bool) -> Bool`
**Description:** Logical XOR of two booleans.

---

## length
**Category:** list
**Signature:** `(list : List|Bytes) -> Int`
**Description:** Number of elements in a list or bytes buffer.

---

## at
**Category:** list
**Signature:** `(list : List) (index : Int) -> Any`
**Description:** Element at zero-based index. Panics if out of bounds.

---

## at-or
**Category:** list
**Signature:** `(list : List) (index : Int) (default : Any) -> Any`
**Description:** Element at index, or default if index is out of bounds.

---

## set-at
**Category:** list
**Signature:** `(list : List) (index : Int) (value : Any) -> List`
**Description:** Return a new list with the element at index replaced by value.

---

## append
**Category:** list
**Signature:** `(list : List) (item : Any) -> List`
**Description:** Return a new list with item appended to the end.

---

## head
**Category:** list
**Signature:** `(list : List) -> Any`
**Description:** First element of a list. Panics on empty list.

---

## tail
**Category:** list
**Signature:** `(list : List) -> List`
**Description:** All elements except the first. Panics on empty list.

---

## empty?
**Category:** list
**Signature:** `(list : List) -> Bool`
**Description:** True if the list has no elements.

---

## cons
**Category:** list
**Signature:** `(item : Any) (list : List) -> List`
**Description:** Prepend item to the front of list.

---

## list-contains?
**Category:** list
**Signature:** `(list : List) (item : Any) -> Bool`
**Description:** True if list contains an element structurally equal to item.

---

## split
**Category:** string
**Signature:** `(s : String) (sep : String) -> List`
**Description:** Split string s on separator sep, returning a list of substrings.

---

## join
**Category:** string
**Signature:** `(list : List) (sep : String) -> String`
**Description:** Join a list of strings with sep as the separator.

---

## substring
**Category:** string
**Signature:** `(s : String) (start : Int) (end : Int) -> String`
**Description:** Substring of s from byte offset start (inclusive) to end (exclusive).

---

## replace
**Category:** string
**Signature:** `(s : String) (from : String) (to : String) -> String`
**Description:** Replace the first occurrence of from in s with to.

---

## char-at
**Category:** string
**Signature:** `(s : String) (index : Int) -> String`
**Description:** Single-character string at the given index (character position, not byte offset).

---

## char-count
**Category:** string
**Signature:** `(s : String) -> Int`
**Description:** Number of Unicode characters (code points) in s.

---

## char-code
**Category:** string
**Signature:** `(s : String) -> Int`
**Description:** Unicode code point of the first character of a single-character string.

---

## char-from-code
**Category:** string
**Signature:** `(code : Int) -> String`
**Description:** Single-character string from a Unicode code point.

---

## chars
**Category:** string
**Signature:** `(s : String) -> List`
**Description:** Explode string into a list of single-character strings.

---

## char-upper?
**Category:** string
**Signature:** `(c : String) -> Bool`
**Description:** True if the single-character string c is an uppercase letter.

---

## char-lower?
**Category:** string
**Signature:** `(c : String) -> Bool`
**Description:** True if the single-character string c is a lowercase letter.

---

## string-ci=?
**Category:** string
**Signature:** `(a : String) (b : String) -> Bool`
**Description:** Case-insensitive string equality.

---

## str
**Category:** string
**Signature:** `(args : Any...) -> String`
**Description:** Convert one or more values to their string representations and concatenate.

---

## format
**Category:** string
**Signature:** `(template : String) (args : Any...) -> String`
**Description:** Printf-style string formatting. Supports %s, %d, %f, %x, %b, %%.

---

## map-new
**Category:** map
**Signature:** `() -> Map`
**Description:** Create a new empty map.

---

## map-get
**Category:** map
**Signature:** `(map : Map) (key : Any) -> Any`
**Description:** Get the value for key in map. Returns nil if the key is absent.

---

## map-set
**Category:** map
**Signature:** `(map : Map) (key : Any) (value : Any) -> Map`
**Description:** Return a new map with key bound to value.

---

## map-has
**Category:** map
**Signature:** `(map : Map) (key : Any) -> Bool`
**Description:** True if map contains the given key.

---

## map-remove
**Category:** map
**Signature:** `(map : Map) (key : Any) -> Map`
**Description:** Return a new map with the given key removed.

---

## map-keys
**Category:** map
**Signature:** `(map : Map) -> List`
**Description:** Return a list of all keys in map.

---

## sqrt
**Category:** math
**Signature:** `(x : Float) -> Float`
**Description:** Square root of x.

---

## sin
**Category:** math
**Signature:** `(x : Float) -> Float`
**Description:** Sine of x (radians).

---

## cos
**Category:** math
**Signature:** `(x : Float) -> Float`
**Description:** Cosine of x (radians).

---

## tan
**Category:** math
**Signature:** `(x : Float) -> Float`
**Description:** Tangent of x (radians).

---

## log
**Category:** math
**Signature:** `(x : Float) -> Float`
**Description:** Natural logarithm of x.

---

## exp
**Category:** math
**Signature:** `(x : Float) -> Float`
**Description:** e raised to the power x.

---

## floor
**Category:** math
**Signature:** `(x : Float) -> Float`
**Description:** Largest integer not greater than x, as Float.

---

## ceil
**Category:** math
**Signature:** `(x : Float) -> Float`
**Description:** Smallest integer not less than x, as Float.

---

## round
**Category:** math
**Signature:** `(x : Float) -> Float`
**Description:** Round x to the nearest integer, as Float. Ties go away from zero.

---

## int-to-float
**Category:** math
**Signature:** `(n : Int) -> Float`
**Description:** Convert integer n to a floating-point number.

---

## float-to-int
**Category:** math
**Signature:** `(x : Float) -> Int`
**Description:** Truncate floating-point x to an integer (toward zero).

---

## infinity
**Category:** math
**Signature:** `() -> Float`
**Description:** Positive IEEE 754 infinity.

---

## nan
**Category:** math
**Signature:** `() -> Float`
**Description:** IEEE 754 NaN (not-a-number).

---

## is-nan?
**Category:** math
**Signature:** `(x : Float) -> Bool`
**Description:** True if x is NaN.

---

## is-infinite?
**Category:** math
**Signature:** `(x : Float) -> Bool`
**Description:** True if x is positive or negative infinity.

---

## type-of
**Category:** type
**Signature:** `(v : Any) -> String`
**Description:** Return the runtime type name of v as a string (e.g. "Int", "String", "List").

---

## valid
**Category:** type
**Signature:** `(v : Any) -> Bool`
**Description:** True if v is not nil (and not an error variant).

---

## int-to-string
**Category:** type
**Signature:** `(n : Int) -> String`
**Description:** Decimal string representation of integer n.

---

## float-to-string
**Category:** type
**Signature:** `(x : Float) -> String`
**Description:** String representation of floating-point x.

---

## string-to-int
**Category:** type
**Signature:** `(s : String) -> Int`
**Description:** Parse decimal integer from string s. Panics on invalid input.

---

## string-to-float
**Category:** type
**Signature:** `(s : String) -> Float`
**Description:** Parse floating-point number from string s. Panics on invalid input.

---

## int-to-string-radix
**Category:** type
**Signature:** `(n : Int) (radix : Int) -> String`
**Description:** Convert n to a string in the given radix (2–36).

---

## parse-int-radix
**Category:** type
**Signature:** `(s : String) (radix : Int) -> Int`
**Description:** Parse string s as an integer in the given radix (2–36).

---

## panic
**Category:** type
**Signature:** `(msg : String) -> Never`
**Description:** Abort execution with an error message.

---

## assert
**Category:** type
**Signature:** `(condition : Bool) (msg : String) -> Nil`
**Description:** Abort with msg if condition is false.

---

## print
**Category:** io
**Signature:** `(v : Any) -> Nil`
**Description:** Print v to stdout without a trailing newline.

---

## println
**Category:** io
**Signature:** `(v : Any) -> Nil`
**Description:** Print v to stdout followed by a newline.

---

## eprint
**Category:** io
**Signature:** `(v : Any) -> Nil`
**Description:** Print v to stderr without a trailing newline.

---

## eprintln
**Category:** io
**Signature:** `(v : Any) -> Nil`
**Description:** Print v to stderr followed by a newline.

---

## write-file
**Category:** io
**Signature:** `(path : String) (content : String|Bytes) -> Nil`
**Description:** Write content to the file at path, creating or truncating it.

---

## append-file
**Category:** io
**Signature:** `(path : String) (content : String|Bytes) -> Nil`
**Description:** Append content to the file at path, creating it if necessary.

---

## delete-file
**Category:** io
**Signature:** `(path : String) -> Nil`
**Description:** Delete the file at path.

---

## rename-file
**Category:** io
**Signature:** `(src : String) (dst : String) -> Nil`
**Description:** Rename (move) the file at src to dst.

---

## file-exists?
**Category:** io
**Signature:** `(path : String) -> Bool`
**Description:** True if a file or directory exists at path.

---

## file-size
**Category:** io
**Signature:** `(path : String) -> Int`
**Description:** Size of the file at path in bytes.

---

## file-mtime
**Category:** io
**Signature:** `(path : String) -> Int`
**Description:** Last-modified time of the file at path as Unix timestamp (seconds).

---

## exec-file
**Category:** io
**Signature:** `(path : String) -> Nil`
**Description:** Execute the file at path (exec, replaces current process).

---

## read-dir
**Category:** io
**Signature:** `(path : String) -> List`
**Description:** List the entries of directory at path as a list of filename strings.

---

## create-dir
**Category:** io
**Signature:** `(path : String) -> Nil`
**Description:** Create a directory (and any missing parents) at path.

---

## delete-dir
**Category:** io
**Signature:** `(path : String) -> Nil`
**Description:** Recursively delete the directory at path.

---

## is-dir?
**Category:** io
**Signature:** `(path : String) -> Bool`
**Description:** True if path is an existing directory.

---

## temp-file
**Category:** io
**Signature:** `(suffix : String) -> String`
**Description:** Create a temporary file with the given suffix and return its path.

---

## temp-dir
**Category:** io
**Signature:** `(prefix : String) -> String`
**Description:** Create a temporary directory with the given prefix and return its path.

---

## read-line
**Category:** io
**Signature:** `() -> String`
**Description:** Read one line from stdin (blocking). Returns the line without trailing newline.

---

## read-lines
**Category:** io
**Signature:** `(path : String) -> List`
**Description:** Read all lines of a text file at path as a list of strings.

---

## read-stdin
**Category:** io
**Signature:** `() -> String`
**Description:** Read all of stdin as a string.

---

## get-cwd
**Category:** io
**Signature:** `() -> String`
**Description:** Return the current working directory as a string.

---

## sleep
**Category:** system
**Signature:** `(ms : Int) -> Nil`
**Description:** Sleep for ms milliseconds.

---

## time-now
**Category:** system
**Signature:** `() -> Int`
**Description:** Current Unix timestamp in milliseconds.

---

## cpu-count
**Category:** system
**Signature:** `() -> Int`
**Description:** Number of logical CPU cores available.

---

## format-time
**Category:** system
**Signature:** `(timestamp_ms : Int) (fmt : String) -> String`
**Description:** Format a Unix timestamp (milliseconds) using a strftime-style format string.

---

## shell-exec
**Category:** system
**Signature:** `(cmd : String) (stdin : String) -> Map`
**Description:** Run cmd in a shell, passing stdin as input. Returns a map with keys stdout, stderr, exit_code.

---

## shell-exec-with-stdin
**Category:** system
**Signature:** `(cmd : String) (args : List) (stdin : String) -> Map`
**Description:** Run cmd with explicit args list and stdin. Returns a map with keys stdout, stderr, exit_code.

---

## regex-match
**Category:** regex
**Signature:** `(pattern : String) (s : String) -> Bool`
**Description:** True if the regular expression pattern matches anywhere in s.

---

## regex-find-all
**Category:** regex
**Signature:** `(pattern : String) (s : String) -> List`
**Description:** Return all non-overlapping matches of pattern in s as a list of strings.

---

## regex-replace
**Category:** regex
**Signature:** `(pattern : String) (replacement : String) (s : String) -> String`
**Description:** Replace the first match of pattern in s with replacement. Supports $1 capture groups.

---

## regex-split
**Category:** regex
**Signature:** `(pattern : String) (s : String) -> List`
**Description:** Split s on every match of the regular expression pattern.

---

## bytes-alloc
**Category:** bytes
**Signature:** `(n : Int) -> Bytes`
**Description:** Allocate a zero-filled byte buffer of length n.

---

## bytes-new
**Category:** bytes
**Signature:** `() -> Bytes`
**Description:** Create a new empty byte buffer.

---

## bytes-get
**Category:** bytes
**Signature:** `(buf : Bytes) (index : Int) -> Int`
**Description:** Return the byte value at index as an integer (0–255).

---

## bytes-set!
**Category:** bytes
**Signature:** `(buf : Bytes) (index : Int) (value : Int) -> Nil`
**Description:** Set the byte at index to value (0–255) in-place.

---

## bytes-length
**Category:** bytes
**Signature:** `(buf : Bytes) -> Int`
**Description:** Number of bytes in the buffer.

---

## bytes-from-string
**Category:** bytes
**Signature:** `(s : String) -> Bytes`
**Description:** Convert a UTF-8 string to a byte buffer.

---

## bytes-to-string
**Category:** bytes
**Signature:** `(buf : Bytes) (start : Int) (end : Int) -> String`
**Description:** Decode a slice of buf as a UTF-8 string from byte offset start to end.

---

## bytes-concat
**Category:** bytes
**Signature:** `(a : Bytes) (b : Bytes) -> Bytes`
**Description:** Concatenate two byte buffers.

---

## bytes-concat-all
**Category:** bytes
**Signature:** `(list : List) -> Bytes`
**Description:** Concatenate a list of byte buffers into one.

---

## bytes-slice
**Category:** bytes
**Signature:** `(buf : Bytes) (start : Int) (end : Int) -> Bytes`
**Description:** Return a new byte buffer containing buf[start..end].

---

## bytes-from-int8
**Category:** bytes
**Signature:** `(n : Int) -> Bytes`
**Description:** 1-byte buffer containing n as a signed 8-bit integer.

---

## bytes-from-int16
**Category:** bytes
**Signature:** `(n : Int) -> Bytes`
**Description:** 2-byte buffer containing n as a little-endian signed 16-bit integer.

---

## bytes-from-int32
**Category:** bytes
**Signature:** `(n : Int) -> Bytes`
**Description:** 4-byte buffer containing n as a little-endian signed 32-bit integer.

---

## bytes-from-int64
**Category:** bytes
**Signature:** `(n : Int) -> Bytes`
**Description:** 8-byte buffer containing n as a little-endian signed 64-bit integer.

---

## bytes-to-int16
**Category:** bytes
**Signature:** `(buf : Bytes) (offset : Int) -> Int`
**Description:** Read a little-endian signed 16-bit integer from buf at byte offset.

---

## bytes-to-int32
**Category:** bytes
**Signature:** `(buf : Bytes) (offset : Int) -> Int`
**Description:** Read a little-endian signed 32-bit integer from buf at byte offset.

---

## bytes-to-int64
**Category:** bytes
**Signature:** `(buf : Bytes) (offset : Int) -> Int`
**Description:** Read a little-endian signed 64-bit integer from buf at byte offset.

---

## bytes-xor
**Category:** bytes
**Signature:** `(a : Bytes) (b : Bytes) -> Bytes`
**Description:** XOR two equal-length byte buffers element-wise.

---

## bytes-xor-scalar
**Category:** bytes
**Signature:** `(buf : Bytes) (byte : Int) -> Bytes`
**Description:** XOR every byte in buf with the scalar value byte.

---

## sha256
**Category:** crypto
**Signature:** `(data : String) -> String`
**Description:** Hex-encoded SHA-256 digest of a UTF-8 string.

---

## sha512
**Category:** crypto
**Signature:** `(data : String) -> String`
**Description:** Hex-encoded SHA-512 digest of a UTF-8 string.

---

## hmac-sha256
**Category:** crypto
**Signature:** `(key : String) (data : String) -> String`
**Description:** Hex-encoded HMAC-SHA-256 of data using key.

---

## hmac-sha512
**Category:** crypto
**Signature:** `(key : String) (data : String) -> String`
**Description:** Hex-encoded HMAC-SHA-512 of data using key.

---

## sha256-bytes
**Category:** crypto
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Raw 32-byte SHA-256 digest of a byte buffer.

---

## sha512-bytes
**Category:** crypto
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Raw 64-byte SHA-512 digest of a byte buffer.

---

## hmac-sha256-bytes
**Category:** crypto
**Signature:** `(key : Bytes) (data : Bytes) -> Bytes`
**Description:** Raw 32-byte HMAC-SHA-256 of data using key bytes.

---

## hmac-sha512-bytes
**Category:** crypto
**Signature:** `(key : Bytes) (data : Bytes) -> Bytes`
**Description:** Raw 64-byte HMAC-SHA-512 of data using key bytes.

---

## pbkdf2-sha512
**Category:** crypto
**Signature:** `(password : Bytes) (salt : Bytes) (iterations : Int) (key_len : Int) -> Bytes`
**Description:** PBKDF2 key derivation using HMAC-SHA-512.

---

## random-bytes
**Category:** crypto
**Signature:** `(n : Int) -> Bytes`
**Description:** Cryptographically secure random byte buffer of length n.

---

## crc32c
**Category:** crypto
**Signature:** `(data : Bytes) -> Int`
**Description:** CRC-32C (Castagnoli) checksum of a byte buffer.

---

## bitwise-and
**Category:** bitwise
**Signature:** `(a : Int) (b : Int) -> Int`
**Description:** Bitwise AND of two integers.

---

## bitwise-or
**Category:** bitwise
**Signature:** `(a : Int) (b : Int) -> Int`
**Description:** Bitwise OR of two integers.

---

## bitwise-xor
**Category:** bitwise
**Signature:** `(a : Int) (b : Int) -> Int`
**Description:** Bitwise XOR of two integers.

---

## bitwise-shl
**Category:** bitwise
**Signature:** `(a : Int) (shift : Int) -> Int`
**Description:** Left-shift a by shift bits.

---

## bitwise-shr
**Category:** bitwise
**Signature:** `(a : Int) (shift : Int) -> Int`
**Description:** Arithmetic right-shift a by shift bits.

---

## thread-spawn
**Category:** misc
**Signature:** `(fn : (fn [] -> Any)) -> Int`
**Description:** Spawn a new thread running fn (a zero-argument closure) and return its handle.

---

## thread-join
**Category:** misc
**Signature:** `(handle : Int) -> Any`
**Description:** Block until the thread with handle completes and return its result.

---

## thread-set-affinity
**Category:** misc
**Signature:** `(cpu : Int) -> Nil`
**Description:** Pin the calling thread to logical CPU cpu.

---

## channel-new
**Category:** misc
**Signature:** `() -> Channel`
**Description:** Create a new unbounded MPSC channel.

---

## channel-send
**Category:** misc
**Signature:** `(ch : Channel) (value : Any) -> Nil`
**Description:** Send a value into channel ch.

---

## channel-recv
**Category:** misc
**Signature:** `(ch : Channel) -> Any`
**Description:** Receive the next value from channel ch, blocking until one is available.

---

## channel-recv-timeout
**Category:** misc
**Signature:** `(ch : Channel) (timeout_ms : Int) -> Any`
**Description:** Receive from ch with a timeout in milliseconds. Returns nil on timeout.

---

## channel-drain
**Category:** misc
**Signature:** `(ch : Channel) -> List`
**Description:** Non-blocking drain: return all currently queued values as a list.

---

## channel-close
**Category:** misc
**Signature:** `(ch : Channel) -> Nil`
**Description:** Close channel ch.

---

## dns-resolve
**Category:** misc
**Signature:** `(hostname : String) -> List`
**Description:** Resolve hostname to a list of IP address strings.

---

## icmp-ping
**Category:** misc
**Signature:** `(host : String) (timeout_ms : Int) -> Bool`
**Description:** Send an ICMP echo request to host. Returns true if a reply is received within timeout.

---

## tcp-connect
**Category:** misc
**Signature:** `(host : String) (port : Int) -> TcpSocket`
**Description:** Open a TCP connection to host:port.

---

## tcp-close
**Category:** misc
**Signature:** `(sock : TcpSocket) -> Nil`
**Description:** Close a TCP socket.

---

## tcp-send
**Category:** misc
**Signature:** `(sock : TcpSocket) (data : Bytes) -> Nil`
**Description:** Send data over a TCP socket.

---

## tcp-recv
**Category:** misc
**Signature:** `(sock : TcpSocket) (max_bytes : Int) -> Bytes`
**Description:** Receive up to max_bytes from a TCP socket.

---

## tcp-recv-exact
**Category:** misc
**Signature:** `(sock : TcpSocket) (n : Int) -> Bytes`
**Description:** Receive exactly n bytes from a TCP socket, blocking until available.

---

## tcp-set-timeout
**Category:** misc
**Signature:** `(sock : TcpSocket) (timeout_ms : Int) -> Nil`
**Description:** Set read/write timeout on a TCP socket in milliseconds.

---

## tcp-listen
**Category:** misc
**Signature:** `(host : String) (port : Int) -> TcpListener`
**Description:** Bind a TCP listener to host:port.

---

## tcp-accept
**Category:** misc
**Signature:** `(listener : TcpListener) -> TcpSocket`
**Description:** Accept the next incoming TCP connection, blocking.

---

## gzip-compress
**Category:** misc
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Compress data with gzip.

---

## gzip-decompress
**Category:** misc
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Decompress gzip-compressed data.

---

## snappy-compress
**Category:** misc
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Compress data with Snappy.

---

## snappy-decompress
**Category:** misc
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Decompress Snappy-compressed data.

---

## lz4-compress
**Category:** misc
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Compress data with LZ4.

---

## lz4-decompress
**Category:** misc
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Decompress LZ4-compressed data.

---

## zstd-compress
**Category:** misc
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Compress data with Zstandard.

---

## zstd-decompress
**Category:** misc
**Signature:** `(data : Bytes) -> Bytes`
**Description:** Decompress Zstandard-compressed data.

---

## fn-metadata
**Category:** misc
**Signature:** `(fn : Any) -> Map`
**Description:** Return a map of metadata (name, arity, source) about a function value.

---

## compile-to-executable
**Category:** misc
**Signature:** `(sources : List) (output : String) -> Nil`
**Description:** Compile a list of AIRL source file paths to a standalone executable.

---

## run-bytecode
**Category:** misc
**Signature:** `(bytecode : Bytes) -> Any`
**Description:** Execute pre-compiled AIRL bytecode and return its result.

---

## ash-install-sigint
**Category:** misc
**Signature:** `() -> Nil`
**Description:** Install a SIGINT handler for the ash REPL (captures Ctrl-C without exiting).

---

## ash-sigint-pending
**Category:** misc
**Signature:** `() -> Bool`
**Description:** True if a SIGINT has been received since the last call to this function.

---

