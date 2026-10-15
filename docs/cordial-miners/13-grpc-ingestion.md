# gRPC block ingestion

The adapter has two block input paths in
`crates/cordial-f1r3node-adapter/src/grpc_ingest.rs`. They use different hash
and signature domains and must not be mixed.

## Native f1r3node blocks

`GrpcBlockMapper::from_protobuf` accepts a real f1r3node `BlockMessage` without
parents. For a block with parents, call `from_protobuf_with_parents` with a
`NativeBlockIdentityIndex` built from previously accepted native blocks.

The mapper:

1. Recomputes the host wire hash with `casper::rust::util::proto_util::hash_block`
   and checks the message's declared `block_hash`.
2. Verifies the host signature against that wire hash and the sender key.
3. Translates the host body into a Cordial payload. The translated payload can
   omit host execution details, so its Cordial content hash is a separate hash.
4. Resolves every native parent and justification hash through the accepted
   index to a complete Cordial `BlockIdentity`. An unknown parent is an error.
   A justification that names the wrong creator is also an error.
5. Recomputes the Cordial content hash after replacing parent identities,
   because the predecessor set is part of Cordial block content.

The returned `ValidatedF1r3nodeBlock` carries the wire hash and wire signature
separately from `block.identity`. The translated Cordial identity has an empty
signature: the host signature proves the wire hash, not the Cordial content
hash. Insert it only through a boundary that has already checked the host
signature. A normal Cordial signature verifier cannot validate that empty
identity signature.

Call `NativeBlockIdentityIndex::check_candidate` before inserting a translated
block into the Cordial blocklace. After acceptance, call `record_accepted`
before translating children. The checks reject a wire hash remapped to another
Cordial identity and two distinct wire blocks that collapse to the same
Cordial identity. Checking before insertion avoids changing the blocklace on
a collision.

```text
signed host block -> verify host hash and signature
                  -> resolve accepted native parent hashes
                  -> compute Cordial content hash
                  -> check identity mapping for collisions
                  -> accept Cordial block
                  -> record wire hash to Cordial identity
```

The index is currently in memory. A restart loses it. Persisting the mapping
and wiring this path into `LiveIngress` are later migration slices. If a block
arrives before its parents, the caller must fetch or buffer those native
parents; the mapper will not discard missing ancestry.

## Adapter-owned blocks

`GrpcBlockMapper::from_adapter_message` handles locally constructed adapter
`BlockMessage` values. Their signatures cover Cordial's internal content hash.
Their declared hash can be the adapter snapshot hash or that internal content
hash. This path does not accept real host wire messages.

`LiveIngress::ingest_block_message` currently uses this adapter-owned path.
Real native blocks require the verified path and accepted parent mapping above
before they can be connected to the live mirror.

## Verification

Run the focused tests with:

```bash
cargo test -j 1 -p cordial-f1r3node-adapter --test test_grpc_ingest --offline
```

The signed parent–child test checks native hash verification, parent lookup,
Cordial predecessor closure, content rehashing, and justification creator
matching. A separate test checks that distinct host blocks cannot share one
translated Cordial identity.
