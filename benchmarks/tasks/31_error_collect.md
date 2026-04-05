Write a function `validate-record` that takes a map with keys "name", "age", "email" and validates all three fields, collecting ALL errors (not just the first).

Requirements:
- name must be non-empty string
- age must be integer between 0 and 150
- email must contain "@"
- Return a list of error strings (empty list = valid)

Print the errors for {"name" "" "age" 200 "email" "notanemail"}
