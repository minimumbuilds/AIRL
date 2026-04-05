Write a function `find-senior-dept` that takes a list of employee maps (each with "name", "age", "dept") and returns the department of the first employee over 60, or "none" if no such employee exists.

Requirements:
- Use functional style (no mutation)
- Return "none" if list is empty or no employee over 60

---TESTS---
find-senior-dept([{"name" "Alice" "age" 65 "dept" "Engineering"}]) => Engineering
find-senior-dept([{"name" "Bob" "age" 30 "dept" "Sales"}]) => none
find-senior-dept([]) => none
