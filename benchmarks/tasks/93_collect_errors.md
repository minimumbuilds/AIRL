Write a function called `collect-errors` that takes a list of Result values and returns a list of all error messages.

Requirements:
- Filter the list to keep only Err variants
- Extract the error message from each Err
- Return a list of error message strings
- Ok values are skipped
- An empty list returns an empty list

Test case: collect-errors([(Ok 1), (Err "bad"), (Ok 2), (Err "fail")]) should return ["bad", "fail"]
Test case: collect-errors([(Ok 1), (Ok 2)]) should return []

Print the result of calling the function with [(Ok 10), (Err "not found"), (Ok 20), (Err "timeout"), (Err "denied")].
