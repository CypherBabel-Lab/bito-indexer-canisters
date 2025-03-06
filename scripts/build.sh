#!/usr/bin/env bash

CanisterID="indexer"
echo "building $CanisterID"
dfx build $CanisterID
echo "generating did file"
source ./scripts/did.sh