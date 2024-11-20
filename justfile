set dotenv-load
alias ld := local-down

# Path and Variables
AUTHOR := "ivanshyu"
PROJECT := "btc-idx-service"
REPO := "https://github.com" / AUTHOR / PROJECT
ROOT_DIR := justfile_directory()
SEM_VER := `awk -F' = ' '$1=="version"{print $2;exit;}' Cargo.toml`

default:
    @just --choose

###########################################################
### Build 

# Build release bis binary
build:
	cargo build --bin bis --release

# Build debug bis binary
build-debug:
	cargo build --bin bis 

###########################################################
### Docker

# docker platform='linux/amd64':
#     # docker buildx --platform linux/amd64,linux/arm64 
#     docker buildx build --progress plain --platform {{platform}} \
#     --secret id=gitconfig,src=${HOME}/.gitconfig \
#     --secret id=git-credentials,src=${HOME}/.git-credentials \
#     -t gcr.io/alpha-carbon/swap:v{{SEM_VER}} \
#     -t gcr.io/alpha-carbon/swap:latest \
#     -f docker/Dockerfile .

# docker-push:
#     docker push gcr.io/alpha-carbon/swap -a
#     docker image prune -f

# docker-release: docker docker-push
 
###########################################################
###########################################################
### Local Deployment

local-pull:
    docker compose -f ./deployment/docker-compose.yaml pull
    docker image prune -f

local-dev:
	just local-pg local-testnets

local-mono:
    RUST_BACKTRACE=1 RUST_LOG=info,sqlx=info cargo run --bin bis scan-block

local-pg:
	docker compose -f ./deployment/docker-compose.yaml up -d postgres adminer 

local-testnet:
	test -d ~/bitcoin-data/testnet || mkdir -p ~/bitcoin-data/testnet
	docker compose -f ./deployment/docker-compose.yaml up -d btc-testnet
	
local-reg:
	test -d ~/bitcoin-data/regtest || mkdir -p ~/bitcoin-data/regtest
	docker compose -f ./deployment/docker-compose.yaml up -d btc-regtest

local-down:
	docker compose -f ./deployment/docker-compose.yaml down
