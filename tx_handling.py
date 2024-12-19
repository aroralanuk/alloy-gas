import json

# Define the fields we want to keep
fields_to_keep = {
    "block_number",
    "transaction_index",
    "from_address",
    "gas_limit",
    "gas_used",
    "gas_price"
}

def clean_transaction_data(input_file, output_file):
    # Read the JSON data from the input file
    with open(input_file, 'r') as file:
        data = json.load(file)
    
    # Filter each transaction to keep only the necessary fields
    cleaned_data = [
        {key: transaction[key] for key in transaction if key in fields_to_keep}
        for transaction in data
    ]
    
    # Write the cleaned data to the output file
    with open(output_file, 'w') as file:
        json.dump(cleaned_data, file, indent=2)

# Specify the input and output file paths
input_file = 'data/transactions_1000.json'
output_file = 'data/cleaned_transactions_1000.json'

# Clean the transaction data
clean_transaction_data(input_file, output_file)