# bito-indexer-canisters

An onchain indexer(ord) on the Internet Computer

## Deploy

### bitcoin-rpc-proxy
Due to Bitcoin RPC `getblocks` responses exceeding the 2MB `max_response_bytes` limit of HTTPS outcalls, this component:
- Implements [Range requests](https://developer.mozilla.org/en-US/docs/Web/HTTP/Range_requests)
- Provides a wrapper for Bitcoin RPC to support range requests

### indexer canister
```bash
./scripts/build.sh
dfx deploy indexer
```

### API

All the API are in `indexer.did`, you can use `dfx canister call indexer xxx` to call them.