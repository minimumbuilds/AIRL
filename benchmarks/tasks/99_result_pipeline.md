Write a pipeline of three functions that each return Result, chained together so any failure short-circuits.

Requirements:
- `step1`: Takes an integer, returns (Err "negative") if < 0, otherwise (Ok (* n 2))
- `step2`: Takes an integer, returns (Err "too large") if > 100, otherwise (Ok (+ n 10))
- `step3`: Takes an integer, returns (Ok (int-to-string n))
- Chain: step1 -> step2 -> step3 using nested match
- Write a function `pipeline` that takes an integer and runs all three steps
- No early return — use nested match to propagate errors

Test case: pipeline(5) should return (Ok "20") — step1: 10, step2: 20, step3: "20"
Test case: pipeline(-1) should return (Err "negative")
Test case: pipeline(50) should return (Err "too large") — step1: 100, step2 rejects

Print the result of calling pipeline with 25.
