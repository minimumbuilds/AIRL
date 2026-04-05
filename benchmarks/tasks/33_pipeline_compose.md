Write a function `process-data` that composes these steps: parse integer, validate positive, square it, format as string with prefix "result:".
Each step can fail. Return Ok(string) or Err(reason).

---TESTS---
process-data("4") => Ok(result:16)
process-data("-3") => Err(not positive)
process-data("abc") => Err(not a number)
