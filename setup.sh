#!/bin/bash

apt update
apt install build-essential pkg-config clang libssl-dev screen -y
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly --profile complete -y
source "$HOME/.cargo/env"
source "./garb/.env"

