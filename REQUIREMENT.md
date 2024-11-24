# [Senior Back-end Developer] Bitcoin Indexing Service

- Bitcoin uses UTXO model instead of account-based model. The current Implemented Bitcoin Node does not have such account keeping approach.
- Design and implement a indexing service that stores raw, processed and aggregated data of Bitcoin transactions.
- Provide functioning API listed below

## Basic Requirements

- Understand Bitcoin Ledger, Transaction and UTXO
- Differentiate P2TR and other Bitcoin Locking Scripts
- Design a pipeline that processes Bitcoin ledger (Block ⇒ Transaction ⇒ UTXO ⇒ Balance)
- Design a scalable RDBMS schema
    - Example
        - Block ⇒ Store Block Header
        - Transaction ⇒ Store Bitcoin Transaction
        - UTXO ⇒ Store UTXO
        - P2TR ⇒ Store P2TR and its balance
- Store Aggregated data, such as Snapshot of Balance, if needed
- Use de facto standard Bitcoin JSON RPC (QuickNode free tier) for developing and testing
- **DO NOT use existing 3rd party Bitcoin indexer implementation, such as Electrum / Electrs / etc.**

## Feature Requirements

- Provide a simple README for usage
    - Explain completed features
    - Explain architecture
    - Explain 3rd party libraries choice
    - How to build
    - How to install
    - How to configure
- Should support one of network kinds. Ex: Regtest / Testnet / Mainnet
- Should include a web service and a command-line interface
- Expected Web Service API
    - GET `/api/v1/raw/block/latest`
        - Response
        
        ```tsx
        {
          "latest_block_height": block_height
        }
        ```
        
    - GET `/api/v1/raw/block/:block_hash`
        - Request
            - `block_hash`
        - Response: `block_header_data` could be arbitrary representation
            
            ```tsx
            {
              "block_header_data": block_header_data
            }
            ```
            
    - GET `/api/v1/raw/transaction/:transaction_id`
        - Request
            - `transactoin_id`
        - Response: `transaction_data` could be arbitrary representation
            
            ```tsx
            {
              "transaction_data": transaction_data
            }
            ```
            
    - GET `/api/v1/processed/p2tr/:p2tr_address/balance`
        - Request
            - `p2tr_address` (Should support bech32m encoding)
            - Ex: `/api/v1/processed/p2tr/bc1qrqp9vfakep7wsze7h62crghz7h0kh5ry3ynf5v/balance`
        - Response: `current_balance` should use satoshi as unit
            
            ```tsx
            {
              "curent_balance": current_balance
            }
            ```
            
    - GET `/api/v1/processed/p2tr/:p2tr_address/utxo`
        - Request
            - `p2tr_address` (Should support bech32m encoding)
            - Ex: `/api/v1/processed/p2tr/bc1qrqp9vfakep7wsze7h62crghz7h0kh5ry3ynf5v/utxo`
        - Response
            
            ```tsx
            {
              [
            	  {
            	    "transaction_id": transaction_id,
            	    "transaction_index": transaction_index,
            	    "satoshi": satoshi,
            	    "block_height": block_height,
            	  },
            	  ...
            	]
            }
            ```
            
    - GET `/api/v1/aggregated/p2tr/:p2tr_address?time_span={time_span}&granularity={granularity}`
        - Return snapshots of P2TR balances for a given time span and granularity
        - Request
            - `p2tr_address` (Should support bech32m encoding)
            - `time_span`
                - `m`: Recent Month
                - `w`: Recent Week
                - `d`: Recent Day
            - `granularity`
                - `w`: Weekly
                - `d`: Daily
                - `h`: Hourly
            - Ex: `/api/v1/aggregated/p2tr/bc1qrqp9vfakep7wsze7h62crghz7h0kh5ry3ynf5v?time_span=w&granularity=d`
        - Response
            - Ex: Span 1 Week, Granularity Day
            
            ```tsx
            // Key: Unix Timestamp
            [
              1731772800: [{ "balance": "snapshooted_balance" }], // 2024.11.17
              1731686400: [{ "balance": "snapshooted_balance" }], // 2024.11.16
              1731600000: [{ "balance": "snapshooted_balance" }], // 2024.11.15
              1731513600: [{ "balance": "snapshooted_balance" }], // 2024.11.14
              1731427200: [{ "balance": "snapshooted_balance" }], // 2024.11.13
              1731340800: [{ "balance": "snapshooted_balance" }], // 2024.11.12
              1731254400: [{ "balance": "snapshooted_balance" }], // 2024.11.11
            ]
            ```
            
- Expected CLI
    - `indexer scan-block —-from 123456 -—to 124456`
        - Can scan and process Bitcoin blocks by giving a range of Block heights
    - Use RPC
        - gRPC
        - RESTful API
- Overall Requirements
    - Containerize
    - Testing

## Advanced Requirements

- Automatically indexing new Bitcoin blocks
- Multithreading
- Set up configuration options using configuration file
- View logs on visualization dashboard (Kibana, Grafana, ...)
- Docker Compose
- Kubernetes
- Deploy on cloud computing platforms (AWS, Azure, GCP, ...)
- CI / CD

## References

- ‣
- https://www.investopedia.com/bitcoin-taproot-upgrade-5210039
- https://bitcoin.design/guide/glossary/address/
- https://blockchain-academy.hs-mittweida.de/bech32-tool/

## Programming Language

- Can choose one of the following programming language
    - Rust
    - Golang
    - C / C++
    - Java
    - Scala
    - TypeScript

## Evaluation

- **Code Quality**
    - How you reason about making sure code is readable and maintainable
        - Modularization
        - Naming
        - Error handling
    - How you write commit messages and how you use version control system such as git
- **Testing**
    - How you reason about what to test and how to test it
- **System Design**
    - How you reason about concepts like reusability, separation of concerns and various abstractions
- **Infrastructure and Operations**
    - How you would run a system and what's important to think about

## Submission

- Please commit code to your GitHub account.
- Please inform recruiter (via Email or Telegram) after pushing to Github.
- You must complete basic requirements within 2 weeks.
- You could ask for 1 more week to complete some advance requirements further after finishing basic requirements if you want.