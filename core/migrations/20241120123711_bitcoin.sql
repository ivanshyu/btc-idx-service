
CREATE TABLE IF NOT EXISTS btc_blocks
(
    hash VARCHAR(66) PRIMARY KEY NOT NULL,
    number BIGINT UNIQUE NOT NULL,
    previous_hash VARCHAR(66),
    timestamp TIMESTAMP NOT NULL,
    nonce BIGINT,
    version INT,
    difficulty NUMERIC
);

CREATE INDEX IF NOT EXISTS block_num_idx ON btc_blocks (number);

CREATE TABLE IF NOT EXISTS btc_transactions (
    txid VARCHAR(64) PRIMARY KEY NOT NULL,
    block_hash VARCHAR(66) NOT NULL,
    transaction_index INT NOT NULL,
    lock_time BIGINT,
    is_coinbase BOOLEAN NOT NULL,
    version INT,
    FOREIGN KEY (block_hash) REFERENCES btc_blocks(hash) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS btc_balances
(    
    address VARCHAR(62) PRIMARY KEY NOT NULL,
    balance NUMERIC NOT NULL,
    last_updated TIMESTAMP NOT NULL
);

CREATE TABLE IF NOT EXISTS btc_utxos
(
    -- id is a txid:vout string to batch query relevant utxos
    id VARCHAR(74) NOT NULL,
    address VARCHAR(62) NOT NULL,
    txid VARCHAR(64) NOT NULL REFERENCES btc_transactions(txid) ON DELETE CASCADE,
    vout BIGINT NOT NULL,
    amount NUMERIC NOT NULL,
    block_number BIGINT NOT NULL, 
    -- If NULL, the UTXO is unspent
    spent_block NUMERIC,
    PRIMARY KEY(txid, vout)
);

CREATE INDEX IF NOT EXISTS btc_utxos_id_idx ON btc_utxos (spent_block, id);
CREATE INDEX IF NOT EXISTS btc_utxos_block_num_idx ON btc_utxos (block_number);

CREATE TABLE IF NOT EXISTS btc_p2tr_events
(
    sequence_id BIGSERIAL PRIMARY KEY NOT NULL,
    block_number NUMERIC NOT NULL,
    tx_hash VARCHAR(66) NOT NULL,
    address VARCHAR(62) NOT NULL,
    amount VARCHAR(32) NOT NULL,
    action SMALLINT NOT NULL
);


CREATE TABLE IF NOT EXISTS pending_btc_p2tr_events
(
    block_number NUMERIC NOT NULL,
    tx_hash VARCHAR(66) NOT NULL,
    address VARCHAR(62) NOT NULL,
    amount VARCHAR(32) NOT NULL,
    action SMALLINT NOT NULL    
);

CREATE INDEX IF NOT EXISTS pending_btc_event_idx ON pending_btc_p2tr_events (block_number);
CREATE INDEX IF NOT EXISTS pending_btc_event_addr_idx ON pending_btc_p2tr_events (address);

CREATE TABLE IF NOT EXISTS statistic_btc_balances
(
    address VARCHAR(62) NOT NULL,
    balance NUMERIC NOT NULL,
    datetime_hour TIMESTAMP NOT NULL,
    last_updated TIMESTAMP NOT NULL,
    PRIMARY KEY(address, datetime_hour)
);
