import tabulate

data = [
    {"Name": "Alice", "Age": 30, "City": "New York"},
    {"Name": "Bob", "Age": 25, "City": "Chicago"},
    {"Name": "Eve", "Age": 35, "City": "San Francisco"},
]

print(tabulate.tabulate(data, headers="keys", tablefmt="markdown"))
