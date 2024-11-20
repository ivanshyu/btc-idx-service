
CREATE TABLE IF NOT EXISTS btc_blocks
(
    block_hash VARCHAR(66) PRIMARY KEY NOT NULL,
    block_number BIGINT UNIQUE NOT NULL,
    previous_block_hash VARCHAR(66),
    timestamp TIMESTAMP NOT NULL,
    nonce BIGINT,
    version INT,
    difficulty NUMERIC
);

CREATE INDEX IF NOT EXISTS block_num_idx ON btc_blocks (block_number);

CREATE TABLE IF NOT EXISTS btc_transactions (
    txid VARCHAR(64) PRIMARY KEY NOT NULL,
    block_hash VARCHAR(66) NOT NULL,
    transaction_index INT NOT NULL,
    version INT,
    lock_time BIGINT,
    timestamp TIMESTAMP NOT NULL,
    is_coinbase BOOLEAN NOT NULL,
    FOREIGN KEY (block_hash) REFERENCES btc_blocks(block_hash) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS btc_balances
(    
    address VARCHAR(50) PRIMARY KEY NOT NULL,
    balance NUMERIC NOT NULL,
    last_updated TIMESTAMP NOT NULL
);

CREATE TABLE IF NOT EXISTS btc_utxos
(
    -- id is a txid:vout string to batch query relevant utxos
    id VARCHAR(74) NOT NULL,
    address VARCHAR(50) NOT NULL,
    txid VARCHAR(64) NOT NULL REFERENCES btc_transactions(txid) ON DELETE CASCADE,
    vout BIGINT NOT NULL,
    amount NUMERIC NOT NULL,
    block_number NUMERIC NOT NULL, 
    -- If NULL, the UTXO is unspent
    spent_block NUMERIC,
    PRIMARY KEY(txid, vout)
);

CREATE INDEX IF NOT EXISTS btc_utxos_id_idx ON btc_utxos (spent_block, id);
CREATE INDEX IF NOT EXISTS btc_utxos_block_num_idx ON btc_utxos (block_number);

CREATE TABLE IF NOT EXISTS btc_wallet_events
(
    sequence_id BIGSERIAL PRIMARY KEY NOT NULL,
    block_number NUMERIC NOT NULL,
    tx_hash VARCHAR(66) NOT NULL,
    address VARCHAR(50) NOT NULL,
    amount VARCHAR(32) NOT NULL,
    action SMALLINT NOT NULL
);


CREATE TABLE IF NOT EXISTS pending_btc_wallet_events
(
    block_number NUMERIC NOT NULL,
    tx_hash VARCHAR(66) NOT NULL,
    address VARCHAR(50) NOT NULL,
    amount VARCHAR(32) NOT NULL,
    action SMALLINT NOT NULL    
);

CREATE INDEX IF NOT EXISTS pending_btc_event_idx ON pending_btc_wallet_events (block_number);
CREATE INDEX IF NOT EXISTS pending_btc_event_addr_idx ON pending_btc_wallet_events (address);