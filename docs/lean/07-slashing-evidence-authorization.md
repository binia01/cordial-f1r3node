# Cordial Slashing Evidence and Authorization

Lean module: `LeanVerification/Slashing.lean`

Issue: [#281 — Define the Cordial slashing evidence and authorization model](https://github.com/iCog-Labs-Dev/cordial-f1r3node/issues/281)

## Why the boundaries are separate

A penalty calculation can be numerically correct and still slash the wrong
validator. The formal contract therefore does not let a raw byte vector stand
in for proof. It models four separate stages:

1. raw evidence claims an offender, two blocks, and an offense round;
2. structural validation proves that claim describes a Cordial equivocation;
3. authentication records successful external content-identity and signature
   verification;
4. protocol authorization permits applying the authenticated evidence in a
   particular PoR round.

No theorem in the module derives a later stage from an earlier one.

## Raw and valid evidence

`RawEquivocationEvidence` is deliberately untrusted:

```lean
structure RawEquivocationEvidence where
  claimedOffender : NodeId
  firstBlock : BlockId
  secondBlock : BlockId
  offenseRound : Nat
  proofMaterial : List UInt8
```

The opaque `proofMaterial` can represent serialized blocks, signatures, or
host mapping data. Its presence, length, and contents establish no proposition
in Lean.

`ValidEquivocationEvidence B hV evidence` proves all of the structural facts
needed at the slashing boundary:

- both block identities are present and distinct;
- both blocks have the same creator;
- that creator is the claimed offender;
- neither block observes the other;
- both blocks have the same computed Cordial depth;
- the recorded offense round is that depth;
- the pair satisfies the existing `Equivocation B hV` predicate.

The last field is important: the slashing model reuses the protocol's existing
equivocation definition instead of introducing a competing one.

## Authentication trust assumption

```lean
structure CryptographicVerificationAssumption where
  verifies : Blocklace → RawEquivocationEvidence → Prop

def EvidenceAuthenticated
    (crypto : CryptographicVerificationAssumption)
    (B : Blocklace) (evidence : RawEquivocationEvidence) : Prop :=
  crypto.verifies B evidence
```

`CryptographicVerificationAssumption.verifies` is the module's named trust
assumption. An implementation may cross this boundary only after verifying the
mapping from supplied block contents to their identities and checking both
creators' signatures. The Lean model does not attempt to prove the
cryptographic algorithm itself.

In particular, `EvidenceAuthenticated` does not inspect `proofMaterial` and is
not equivalent to a nonempty-byte check.

## Authorization and round binding

`PenaltyAuthorizationDecision` distinguishes a finalized Cordial anchor from
an abstract finalized PoR attestation. `PenaltyAuthorizationPolicy` is the
protocol-provided relation that states which decision authorizes which evidence
and application round.

`AuthorizedPenaltyEvent` contains:

- structurally valid evidence;
- a separate authentication proof;
- the bound offender and original offense round;
- a distinct PoR penalty application round;
- a proof that application is not before the offense;
- the finalization or attestation decision;
- a proof that the configured protocol policy authorizes the application.

Thus detection, authentication, and authorization remain independent facts.

## Key theorems

```lean
theorem validEvidence_implies_equivocation ... :
  Equivocation B hV evidence.firstBlock evidence.secondBlock
```

Structural validity exposes the exact existing Cordial equivocation carried by
the evidence.

```lean
theorem validEvidence_implies_equivocator ... :
  Equivocator B hV evidence.claimedOffender
```

The validated creator binding turns the block pair into an `Equivocator`
witness for the claimed offender.

```lean
theorem validEvidence_offender_unique ...
    (hSameBlock : first.firstBlock = second.firstBlock) :
  first.claimedOffender = second.claimedOffender
```

Two valid claims sharing a conflicting block cannot attribute that block to
different offenders, because `creatorOf` returns one creator.

```lean
theorem validEvidence_round_bound ... :
  blockDepth B hV evidence.firstBlock = evidence.offenseRound ∧
  blockDepth B hV evidence.secondBlock = evidence.offenseRound
```

The explicit offense round is the computed depth of both conflicting blocks.

```lean
theorem authorizedEvent_contains_validEvidence ... :
  ValidEquivocationEvidence B hV event.evidence
```

Authorization cannot erase or replace the structural proof.

```lean
theorem authorizedEvent_offender_bound ... :
  event.offender = event.evidence.claimedOffender ∧
  creatorOf B event.evidence.firstBlock = some event.offender ∧
  creatorOf B event.evidence.secondBlock = some event.offender
```

The event-level offender is tied to both conflicting block creators.

```lean
theorem authorizedEvent_applicationRound_bound ... :
  event.offenseRound ≤ event.applicationRound ∧
  event.evidence.offenseRound ≤ event.applicationRound
```

The PoR application round is distinct from, and cannot precede, the original
offense round.

## Lean-to-Rust contract mapping

The table describes the contract Rust must eventually satisfy. It does not
claim the current implementation already enforces every boundary.

| Lean definition or theorem | Rust counterpart / obligation |
|---|---|
| `RawEquivocationEvidence` | `consensus::evidence::EquivocationEvidence` raw retained record |
| `ValidEquivocationEvidence` | checks currently split across block validation, round depth, equivocation detection, and adapter evidence validation |
| `validEvidence_implies_equivocation` | `equivocation_blocks_at_round` / `all_equivocations` structural result |
| `validEvidence_implies_equivocator` | offender indexing in `EvidencePool` must agree with the conflicting blocks' creator |
| `validEvidence_round_bound` | `consensus::round::depth` and `EquivocationEvidence::round` correlation |
| `EvidenceAuthenticated` | future verified host-content mapping plus `CryptoVerifier::verify_block` for both blocks |
| `PenaltyAuthorizationDecision` | finalized host block or finalized PoR attestation used to authorize a penalty |
| `AuthorizedPenaltyEvent` | future authenticated input to `ReputationPenaltyEvents` / tiered-slashing application |
| `authorizedEvent_offender_bound` | formatter and host authorization must not accept an offender independent of the signed block creators |
| `authorizedEvent_applicationRound_bound` | PoR transition scheduling must retain the original offense round separately from the application round |

The production formatter currently rejects nonempty evidence when verified host
metadata is unavailable. That fail-closed behavior is compatible with this
contract; it does not yet implement the authentication and authorization proof
objects modeled here.
