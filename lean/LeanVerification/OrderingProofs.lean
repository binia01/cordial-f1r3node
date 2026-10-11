/-
Correctness of the executable τ ordering model (Issue #244).

The executable path is:

  final leaders → previous-final recursion → fresh approved fragments
                → deterministic Kahn sort → τ output

This module proves the observable contracts of that path. Prefix preservation
is deliberately stated for `ValidAppend`, not arbitrary `SubBlocklace`: a DAG
extension may introduce an earlier competing leader or change an old approved
fragment. `ValidAppend` records the protocol facts that rule those cases out.
-/

import LeanVerification.Ordering

namespace CordialMiners

open CMRef

/-! ## Certified topological sort -/

/-- The Boolean topological validator is exactly the semantic contract. -/
theorem checkTopological_iff (parents : ParentMap) (subset order : List BlockId) :
    checkTopological parents subset order = true ↔
      IsTopologicalOrder parents subset order := by
  simp only [checkTopological, IsTopologicalOrder, Bool.and_eq_true,
    decide_eq_true_eq]
  tauto

/-- Kahn sorting only observes parent-map entries for selected blocks. -/
theorem topoLoop_eq_of_parents_eq_on (parents parents' : ParentMap)
    (key : BlockId → String) (subset : List BlockId)
    (hParents : ∀ block, block ∈ subset → parents block = parents' block) :
    ∀ fuel remaining ordered,
      (∀ block, block ∈ remaining → block ∈ subset) →
      topoLoop parents key subset remaining ordered fuel =
        topoLoop parents' key subset remaining ordered fuel := by
  intro fuel
  induction fuel with
  | zero => intro remaining ordered _; rfl
  | succ fuel ih =>
      intro remaining ordered hRemaining
      simp only [topoLoop]
      by_cases hEmpty : remaining.isEmpty
      · simp [hEmpty]
      · simp only [hEmpty]
        simp only [Bool.false_eq_true, if_false]
        have hReady : readyBlocks parents subset ordered remaining =
            readyBlocks parents' subset ordered remaining := by
          unfold readyBlocks
          apply List.filter_congr
          intro block hBlock
          rw [hParents block (hRemaining block hBlock)]
        rw [hReady]
        cases (sortByKey key (readyBlocks parents' subset ordered remaining)).head? with
        | none => rfl
        | some next =>
            apply ih
            intro block hBlock
            exact hRemaining block (List.mem_of_mem_erase hBlock)

/-- Canonical key sorting preserves list membership. -/
theorem mem_insertByKey_iff (key : BlockId → String) (value block : BlockId)
    (values : List BlockId) :
    block ∈ insertByKey key value values ↔ block = value ∨ block ∈ values := by
  induction values with
  | nil => simp [insertByKey]
  | cons head tail ih =>
      simp only [insertByKey]
      split <;> simp [ih, or_left_comm]

theorem mem_sortByKey_iff (key : BlockId → String) (block : BlockId)
    (values : List BlockId) : block ∈ sortByKey key values ↔ block ∈ values := by
  unfold sortByKey
  have hFold : ∀ (rest acc : List BlockId),
      block ∈ rest.foldl (fun sorted value => insertByKey key value sorted) acc ↔
        block ∈ acc ∨ block ∈ rest := by
    intro rest
    induction rest with
    | nil => intro acc; simp
    | cons value tail ih =>
        intro acc
        simp only [List.foldl_cons, ih, mem_insertByKey_iff, List.mem_cons]
        tauto
  simpa using hFold values []

/-- Every successful Kahn-loop output consists of selected blocks when both
the remaining and already ordered lists do. -/
theorem topoLoop_output_subset (parents : ParentMap) (key : BlockId → String)
    (subset : List BlockId) :
    ∀ fuel remaining ordered output,
      (∀ block, block ∈ remaining → block ∈ subset) →
      (∀ block, block ∈ ordered → block ∈ subset) →
      topoLoop parents key subset remaining ordered fuel = .ok output →
      ∀ block, block ∈ output → block ∈ subset := by
  intro fuel
  induction fuel with
  | zero =>
      intro remaining ordered output _ hOrdered hRun
      simp only [topoLoop] at hRun
      split at hRun
      · simpa [Except.ok.inj hRun] using hOrdered
      · simp at hRun
  | succ fuel ih =>
      intro remaining ordered output hRemaining hOrdered hRun
      simp only [topoLoop] at hRun
      split at hRun
      · simpa [Except.ok.inj hRun] using hOrdered
      · rename_i hNotEmpty
        cases hHead : (sortByKey key
            (readyBlocks parents subset ordered remaining)).head? with
        | none => simp [hHead] at hRun
        | some next =>
          simp only [hHead] at hRun
          apply ih (remaining := remaining.erase next) (ordered := ordered ++ [next])
              (output := output)
          · intro block hBlock
            exact hRemaining block (List.mem_of_mem_erase hBlock)
          · intro block hBlock
            rcases List.mem_append.mp hBlock with hOld | hNext
            · exact hOrdered block hOld
            · simp only [List.mem_singleton] at hNext
              subst block
              have hReadyMem : next ∈ readyBlocks parents subset ordered remaining := by
                have hSortedMem : next ∈ sortByKey key
                    (readyBlocks parents subset ordered remaining) := by
                  cases hSorted : sortByKey key
                      (readyBlocks parents subset ordered remaining) with
                  | nil => simp [hSorted] at hHead
                  | cons first rest =>
                      simp [hSorted] at hHead
                      subst first
                      simp
                exact (mem_sortByKey_iff key next _).mp hSortedMem
              unfold readyBlocks at hReadyMem
              exact hRemaining next (List.mem_of_mem_filter hReadyMem)
          · exact hRun

/-- A successful candidate topological sort emits only selected blocks. -/
theorem topoOrder_output_subset (parents : ParentMap) (key : BlockId → String)
    (subset output : List BlockId) (hRun : topoOrder parents key subset = .ok output) :
    ∀ block, block ∈ output → block ∈ subset := by
  unfold topoOrder at hRun
  intro block hBlock
  have hSubset := topoLoop_output_subset parents key subset.eraseDups
    (subset.eraseDups.length + 1) subset.eraseDups [] output
    (fun _ h => h) (by simp) hRun block hBlock
  simpa using hSubset

/-- Deterministic Kahn sorting is unchanged when parent maps agree on every
selected block. -/
theorem topoOrder_eq_of_parents_eq_on (parents parents' : ParentMap)
    (key : BlockId → String) (subset : List BlockId)
    (hParents : ∀ block, block ∈ subset → parents block = parents' block) :
    topoOrder parents key subset = topoOrder parents' key subset := by
  unfold topoOrder
  apply topoLoop_eq_of_parents_eq_on
  · intro block hBlock
    exact hParents block (by simpa using hBlock)
  · intro block hBlock
    exact hBlock

/-- Topological validation is unchanged when parent maps agree on the emitted
order. -/
theorem checkTopological_eq_of_parents_eq_on (parents parents' : ParentMap)
    (subset order : List BlockId)
    (hParents : ∀ block, block ∈ order → parents block = parents' block) :
    checkTopological parents subset order = checkTopological parents' subset order := by
  have hBefore : predecessorsBefore parents subset order =
      predecessorsBefore parents' subset order := by
    unfold predecessorsBefore
    let oldCheck := fun child =>
      match parents child with
      | none => false
      | some predecessors => predecessors.all fun predecessor =>
          if subset.contains predecessor then
            decide (order.idxOf predecessor < order.idxOf child)
          else true
    let newCheck := fun child =>
      match parents' child with
      | none => false
      | some predecessors => predecessors.all fun predecessor =>
          if subset.contains predecessor then
            decide (order.idxOf predecessor < order.idxOf child)
          else true
    have hAll : ∀ xs : List BlockId, (∀ block, block ∈ xs → block ∈ order) →
        xs.all oldCheck = xs.all newCheck := by
      intro xs hXs
      induction xs with
      | nil => rfl
      | cons head tail ih =>
          simp only [List.all_cons]
          have hHead := hParents head (hXs head (by simp))
          simp only [oldCheck, newCheck, hHead]
          rw [ih]
          intro block hBlock
          exact hXs block (by simp [hBlock])
    exact hAll order (fun _ h => h)
  simp [checkTopological, hBefore]

/-- The certified deterministic sort is stable under parent-map agreement on
its selected domain. -/
theorem xsortRef_eq_of_parents_eq_on (parents parents' : ParentMap)
    (key : BlockId → String) (subset : List BlockId)
    (hParents : ∀ block, block ∈ subset → parents block = parents' block) :
    xsortRef parents key subset = xsortRef parents' key subset := by
  unfold xsortRef
  rw [topoOrder_eq_of_parents_eq_on parents parents' key subset hParents]
  cases hSort : topoOrder parents' key subset with
  | error message => rfl
  | ok order =>
      have hOrderParents : ∀ block, block ∈ order →
          parents block = parents' block := by
        intro block hBlock
        exact hParents block
          (topoOrder_output_subset parents' key subset order hSort block hBlock)
      change (if checkTopological parents subset order = true then
          Except.ok order else
          Except.error "internal xsort result failed its topological contract") =
        (if checkTopological parents' subset order = true then
          Except.ok order else
          Except.error "internal xsort result failed its topological contract")
      rw [checkTopological_eq_of_parents_eq_on parents parents' subset order hOrderParents]

/-- Every successful `xsortRef` result satisfies the topological contract. -/
theorem xsortRef_correct (parents : ParentMap) (key : BlockId → String)
    (subset order : List BlockId)
    (h : xsortRef parents key subset = .ok order) :
    IsTopologicalOrder parents subset order := by
  unfold xsortRef at h
  cases hCandidate : topoOrder parents key subset with
  | error message =>
      simp only [hCandidate] at h
      change (Except.error message : Except String (List BlockId)) =
        Except.ok order at h
      exact nomatch h
  | ok candidate =>
      simp only [hCandidate] at h
      change (if checkTopological parents subset candidate = true then
        Except.ok candidate else
        Except.error "internal xsort result failed its topological contract") =
          Except.ok order at h
      split at h
      · rename_i hChecked
        have hEq : candidate = order := Except.ok.inj h
        subst order
        exact (checkTopological_iff parents subset candidate).mp hChecked
      · simp at h

/-- A cyclic selected graph is one admitting no topological order. -/
def CyclicOn (parents : ParentMap) (subset : List BlockId) : Prop :=
  ∀ order, ¬ IsTopologicalOrder parents subset order

/-- `xsortRef` rejects every cyclic selected graph. -/
theorem xsortRef_rejects_cycle (parents : ParentMap) (key : BlockId → String)
    (subset : List BlockId) (hCycle : CyclicOn parents subset) :
    ∃ message, xsortRef parents key subset = .error message := by
  cases hRun : xsortRef parents key subset with
  | error message => exact ⟨message, rfl⟩
  | ok order => exact False.elim (hCycle order (xsortRef_correct _ _ _ _ hRun))

/-- Extract the edge-order fact checked by `predecessorsBefore`. -/
theorem predecessorsBefore_sound (parents : ParentMap) (subset order : List BlockId)
    (child predecessor : BlockId) (predecessors : List BlockId)
    (hChecked : predecessorsBefore parents subset order = true)
    (hChild : child ∈ order) (hParents : parents child = some predecessors)
    (hPredecessor : predecessor ∈ predecessors)
    (hSelected : predecessor ∈ subset) :
    order.idxOf predecessor < order.idxOf child := by
  unfold predecessorsBefore at hChecked
  have hChildChecked := List.all_eq_true.mp hChecked child hChild
  rw [hParents] at hChildChecked
  have hPredecessorChecked :=
    List.all_eq_true.mp hChildChecked predecessor hPredecessor
  have hContains : subset.contains predecessor = true := by simpa using hSelected
  rw [hContains] at hPredecessorChecked
  simp at hPredecessorChecked
  exact hPredecessorChecked

/-! ## τ safety properties -/

/-- A successful τ computation satisfies the whole-output topological
contract. This is independent of the Rust-reported ordering. -/
theorem tauRef_correct
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (key : BlockId → String) (throughWave : ℕ)
    (order : List BlockId)
    (h : tauRef bonds validators B hV wavelength sel domain key throughWave =
      .ok order) :
    IsTopologicalOrder (blockParents B) order order := by
  unfold tauRef at h
  cases hPlan : computeTauPlan bonds validators B hV wavelength sel domain key
      throughWave with
  | error message =>
      simp only [hPlan] at h
      change (Except.error message : Except String (List BlockId)) =
        Except.ok order at h
      exact nomatch h
  | ok result =>
      rcases result with ⟨latest, epochs⟩
      simp only [hPlan] at h
      change (if checkTopological (blockParents B) (flattenEpochs epochs)
          (flattenEpochs epochs) = true then
        Except.ok (flattenEpochs epochs) else
        Except.error "tau output failed duplicate/predecessor validation") =
          Except.ok order at h
      split at h
      · rename_i hChecked
        have hEq : flattenEpochs epochs = order := Except.ok.inj h
        subst order
        exact (checkTopological_iff _ _ _).mp hChecked
      · simp at h

/-- τ is deterministic: successful calls with identical inputs cannot return
different sequences. -/
theorem tauRef_deterministic
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (key : BlockId → String) (throughWave : ℕ)
    {left right : List BlockId}
    (hLeft : tauRef bonds validators B hV wavelength sel domain key throughWave =
      .ok left)
    (hRight : tauRef bonds validators B hV wavelength sel domain key throughWave =
      .ok right) :
    left = right := by
  rw [hLeft] at hRight
  exact Except.ok.inj hRight

/-- τ never returns a duplicate block identity. -/
theorem tauRef_nodup
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (key : BlockId → String) (throughWave : ℕ)
    (order : List BlockId)
    (h : tauRef bonds validators B hV wavelength sel domain key throughWave =
      .ok order) :
    order.Nodup :=
  (tauRef_correct bonds validators B hV wavelength sel domain key throughWave order h).1

/-- If a block and one of its direct predecessors are both emitted, the
predecessor has a strictly smaller output position. -/
theorem tauRef_predecessor_ordered
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (key : BlockId → String) (throughWave : ℕ)
    (order : List BlockId) (child predecessor : BlockId) (block : Block)
    (hRun : tauRef bonds validators B hV wavelength sel domain key throughWave =
      .ok order)
    (hChild : child ∈ order) (hPredecessorOut : predecessor ∈ order)
    (hLookup : B.lookup child = some block)
    (hPredecessor : predecessor ∈ block.content.predecessors) :
    order.idxOf predecessor < order.idxOf child := by
  have hCorrect := tauRef_correct bonds validators B hV wavelength sel domain key
    throughWave order hRun
  have hParents : blockParents B child =
      some (block.content.predecessors.sort (· ≤ ·)) := by
    simp [blockParents, hLookup]
  apply predecessorsBefore_sound (blockParents B) order order child predecessor
      (block.content.predecessors.sort (· ≤ ·)) hCorrect.2.2 hChild hParents
  · simpa using hPredecessor
  · exact hPredecessorOut

/-! ## Prefix preservation under a valid append -/

/-- Earlier finality decisions are unchanged through the old replay horizon.
This is deliberately a
semantic statement about the executable final-leader test, not about the
resulting τ plan. -/
def StableEarlierFinality
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain domain' : List BlockId) (throughWave : ℕ) : Prop :=
  ∀ wave, wave ≤ throughWave →
    finalLeaderAt? bonds validators B hV wavelength sel domain wave =
      finalLeaderAt? bonds validators B' hV' wavelength sel domain' wave

/-- Ratification decisions used while following the previous-final-leader
chain are unchanged for earlier leaders. -/
def StableEarlierRatification
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (domain : List BlockId) : Prop :=
  ∀ leader, leader ∈ domain → ∀ previous,
    checkRatifies bonds validators B hV leader previous =
      checkRatifies bonds validators B' hV' leader previous

/-- Replay-domain growth appends genuinely new, valid block identities to the
complete old domain. -/
structure AppendOnlyDomain (B B' : Blocklace)
    (domain domain' : List BlockId) where
  added : List BlockId
  oldComplete : ∀ block, block ∈ domain ↔ block ∈ B.keys
  extension : domain' = domain ++ added
  addedFresh : ∀ block, block ∈ added → block ∈ B'.keys ∧ block ∉ B.keys

/-- Parent lookup for an old block is unchanged by append-only growth. -/
theorem blockParents_eq_of_subBlocklace {B B' : Blocklace}
    (hsub : SubBlocklace B B') {block : BlockId} (hBlock : block ∈ B.keys) :
    blockParents B block = blockParents B' block := by
  cases hLookup : B.lookup block with
  | none =>
      exact absurd (Finmap.mem_iff.mp (Finmap.mem_keys.mp hBlock)) (by simp [hLookup])
  | some value => simp [blockParents, hLookup, hsub block value hLookup]

/-- Approval filtering for an old leader is derived from immutable old
observations: old targets retain their decision and appended targets cannot be
observed (hence cannot be approved) by that leader. -/
theorem approvedBlocks_stable
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (domain domain' : List BlockId) (hsub : SubBlocklace B B')
    (hDomain : AppendOnlyDomain B B' domain domain')
    (leader : BlockId) (hLeader : leader ∈ domain) (emitted : List BlockId) :
    domain.filter (fun block =>
        checkApproves B hV leader block && !emitted.contains block) =
      domain'.filter (fun block =>
        checkApproves B' hV' leader block && !emitted.contains block) := by
  have hLeaderKey : leader ∈ B.keys := (hDomain.oldComplete leader).mp hLeader
  rw [hDomain.extension, List.filter_append]
  have hOld : domain.filter (fun block =>
        checkApproves B hV leader block && !emitted.contains block) =
      domain.filter (fun block =>
        checkApproves B' hV' leader block && !emitted.contains block) := by
    apply List.filter_congr
    intro block hBlock
    rw [checkApproves_eq_of_subBlocklace hV hV' hsub hLeaderKey
      ((hDomain.oldComplete block).mp hBlock)]
  rw [hOld]
  suffices hAdded : hDomain.added.filter (fun block =>
      checkApproves B' hV' leader block && !emitted.contains block) = [] by
    rw [hAdded, List.append_nil]
  apply List.filter_eq_nil_iff.mpr
  intro block hBlock
  have hFresh := hDomain.addedFresh block hBlock
  rw [checkApproves_new_eq_false hV hV' hsub hLeaderKey hFresh.1 hFresh.2]
  simp

/-- Deterministic topological ordering of every earlier approved fragment is
therefore unchanged; no sorted-output equality is assumed. -/
theorem earlierFragment_stable
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (domain domain' : List BlockId) (key : BlockId → String)
    (hsub : SubBlocklace B B') (hDomain : AppendOnlyDomain B B' domain domain')
    (leader : BlockId) (hLeader : leader ∈ domain) (emitted : List BlockId) :
    xsortRef (blockParents B) key
        (domain.filter fun block =>
          checkApproves B hV leader block && !emitted.contains block) =
      xsortRef (blockParents B') key
        (domain'.filter fun block =>
          checkApproves B' hV' leader block && !emitted.contains block) := by
  let approved := domain.filter fun block =>
    checkApproves B hV leader block && !emitted.contains block
  have hApproved := approvedBlocks_stable B B' hV hV' domain domain' hsub
    hDomain leader hLeader emitted
  change xsortRef (blockParents B) key approved = _
  rw [← hApproved]
  apply xsortRef_eq_of_parents_eq_on
  intro block hBlock
  apply blockParents_eq_of_subBlocklace hsub
  exact (hDomain.oldComplete block).mp (List.mem_of_mem_filter hBlock)

private theorem findSome?_congr_on {X Y : Type} (items : List X)
    (left right : X → Option Y)
    (h : ∀ item, item ∈ items → left item = right item) :
    items.findSome? left = items.findSome? right := by
  induction items with
  | nil => rfl
  | cons head tail ih =>
      simp only [List.findSome?_cons]
      rw [h head (by simp)]
      cases right head
      · apply ih
        intro item hItem
        exact h item (by simp [hItem])
      · rfl

/-- Stable finality and ratification imply stability of the executable
`previousFinalLeader?` search. -/
theorem previousFinalLeader_stable
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain domain' : List BlockId) (throughWave : ℕ)
    (hFinal : StableEarlierFinality bonds validators B B' hV hV'
      wavelength sel domain domain' throughWave)
    (hRatifies : StableEarlierRatification bonds validators B B' hV hV' domain)
    (wave : ℕ) (hWave : wave ≤ throughWave + 1)
    (leader : BlockId) (hLeader : leader ∈ domain) :
    previousFinalLeader? bonds validators B hV wavelength sel domain wave leader =
      previousFinalLeader? bonds validators B' hV' wavelength sel domain' wave leader := by
  unfold previousFinalLeader?
  apply findSome?_congr_on
  intro priorWave hPriorWave
  have hPriorLt : priorWave < wave := by
    rw [List.mem_reverse, List.mem_range] at hPriorWave
    exact hPriorWave
  rw [hFinal priorWave (by omega)]
  cases finalLeaderAt? bonds validators B' hV' wavelength sel domain' priorWave with
  | none => rfl
  | some previous =>
      change (if checkRatifies bonds validators B hV leader previous = true then
          some (priorWave, previous) else none) =
        (if checkRatifies bonds validators B' hV' leader previous = true then
          some (priorWave, previous) else none)
      rw [hRatifies leader hLeader previous]

/-- A leader returned by the executable finality scan came from its replay
domain. -/
theorem finalLeaderAt_mem_domain
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (wave : ℕ) (leader : BlockId)
    (hFinal : finalLeaderAt? bonds validators B hV wavelength sel domain wave =
      some leader) : leader ∈ domain := by
  unfold finalLeaderAt? at hFinal
  generalize hCandidates : domain.filter (fun block =>
      decide (creatorOf B block = sel wave) &&
      decide (blockDepth B hV block = leaderRoundOfWave wave wavelength)) = candidates
    at hFinal
  cases candidates with
  | nil => simp at hFinal
  | cons candidate rest =>
      cases rest with
      | nil =>
          simp only at hFinal
          by_cases hCheck : checkFinal bonds validators B hV wave wavelength sel
              candidate = true
          · simp [hCheck] at hFinal
            have hLeader := hFinal
            subst leader
            apply List.mem_of_mem_filter
            rw [hCandidates]
            simp
          · simp [hCheck] at hFinal
      | cons next tail => simp at hFinal

/-- A successful previous-final result names an earlier wave and a leader from
the same replay domain. -/
theorem previousFinalLeader_lt_and_mem
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (wave : ℕ) (leader : BlockId)
    (priorWave : ℕ) (priorLeader : BlockId)
    (hPrevious : previousFinalLeader? bonds validators B hV wavelength sel
      domain wave leader = some (priorWave, priorLeader)) :
    priorWave < wave ∧ priorLeader ∈ domain := by
  unfold previousFinalLeader? at hPrevious
  rw [List.findSome?_eq_some_iff] at hPrevious
  obtain ⟨before, foundWave, after, hList, hFound, _⟩ := hPrevious
  have hFoundMem : foundWave ∈ (List.range wave).reverse := by
    rw [hList]
    simp
  have hFoundLt : foundWave < wave := by
    simpa using (List.mem_reverse.mp hFoundMem)
  cases hFinal : finalLeaderAt? bonds validators B hV wavelength sel domain foundWave with
  | none => simp [hFinal] at hFound
  | some foundLeader =>
      by_cases hRatifies : checkRatifies bonds validators B hV leader foundLeader = true
      · simp [hFinal, hRatifies] at hFound
        have hWave : foundWave = priorWave := hFound.1
        have hLeader : foundLeader = priorLeader := hFound.2
        subst foundWave
        subst foundLeader
        exact ⟨hFoundLt, finalLeaderAt_mem_domain bonds validators B hV wavelength
          sel domain priorWave priorLeader hFinal⟩
      · simp [hFinal, hRatifies] at hFound

/-- A semantic chain of `previousFinalLeader?` results in the extended
blocklace.  It says that a new latest leader recursively reconnects to the old
latest leader; it contains no τ epochs or output lists. -/
inductive PreviousFinalLeaderChain
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (oldWave : ℕ) (oldLeader : BlockId) :
    ℕ → ℕ → BlockId → Prop where
  | refl : PreviousFinalLeaderChain bonds validators B hV wavelength sel domain
      oldWave oldLeader 0 oldWave oldLeader
  | step {steps wave leader priorWave priorLeader} :
      previousFinalLeader? bonds validators B hV wavelength sel domain wave leader =
        some (priorWave, priorLeader) →
      PreviousFinalLeaderChain bonds validators B hV wavelength sel domain
        oldWave oldLeader steps priorWave priorLeader →
      PreviousFinalLeaderChain bonds validators B hV wavelength sel domain
        oldWave oldLeader (steps + 1) wave leader

/-- Every recursive τ plan rooted at an earlier leader is unchanged by the
growth conditions. -/
theorem tauPlanFromLeader_stable
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain domain' : List BlockId) (key : BlockId → String) (throughWave : ℕ)
    (hsub : SubBlocklace B B') (hDomain : AppendOnlyDomain B B' domain domain')
    (hFinal : StableEarlierFinality bonds validators B B' hV hV'
      wavelength sel domain domain' throughWave)
    (hRatifies : StableEarlierRatification bonds validators B B' hV hV' domain) :
    ∀ fuel wave leader,
      wave ≤ throughWave + 1 → leader ∈ domain →
      tauPlanFromLeader bonds validators B hV wavelength sel domain key
          fuel wave leader =
        tauPlanFromLeader bonds validators B' hV' wavelength sel domain' key
          fuel wave leader := by
  intro fuel
  induction fuel with
  | zero => intro wave leader _ _; rfl
  | succ fuel ih =>
      intro wave leader hWave hLeader
      simp only [tauPlanFromLeader]
      have hPreviousStable := previousFinalLeader_stable bonds validators B B' hV hV'
        wavelength sel domain domain' throughWave hFinal hRatifies wave hWave
        leader hLeader
      rw [hPreviousStable]
      cases hPrevious : previousFinalLeader? bonds validators B' hV' wavelength sel
          domain' wave leader with
      | none =>
          simpa [flattenEpochs] using earlierFragment_stable B B' hV hV' domain
            domain' key hsub hDomain leader hLeader []
      | some previous =>
          rcases previous with ⟨priorWave, priorLeader⟩
          have hPreviousOld : previousFinalLeader? bonds validators B hV wavelength sel
              domain wave leader = some (priorWave, priorLeader) := by
            rw [hPreviousStable]
            exact hPrevious
          have hPrior := previousFinalLeader_lt_and_mem bonds validators B hV
            wavelength sel domain wave leader priorWave priorLeader hPreviousOld
          simp only
          rw [ih priorWave priorLeader (by omega) hPrior.2]
          apply bind_congr
          intro priorEpochs
          exact congrArg
            (Except.map fun suffix =>
              priorEpochs ++ [{ leader := leader, blocks := suffix }])
            (earlierFragment_stable B B' hV hV' domain domain' key hsub hDomain
              leader hLeader (flattenEpochs priorEpochs))

/-- Giving a successful recursive τ computation more fuel does not change
its result. -/
theorem tauPlanFromLeader_fuel_mono
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (key : BlockId → String) :
    ∀ {small large wave leader epochs}, small ≤ large →
      tauPlanFromLeader bonds validators B hV wavelength sel domain key
        small wave leader = .ok epochs →
      tauPlanFromLeader bonds validators B hV wavelength sel domain key
        large wave leader = .ok epochs := by
  intro small
  induction small with
  | zero =>
      intro large wave leader epochs _ hRun
      simp [tauPlanFromLeader] at hRun
  | succ small ih =>
      intro large wave leader epochs hFuel hRun
      cases large with
      | zero => omega
      | succ large =>
          simp only [tauPlanFromLeader] at hRun ⊢
          cases hPrevious : previousFinalLeader? bonds validators B hV wavelength sel
              domain wave leader with
          | none => simpa [hPrevious] using hRun
          | some previous =>
              rcases previous with ⟨priorWave, priorLeader⟩
              simp only [hPrevious] at hRun ⊢
              cases hSmall : tauPlanFromLeader bonds validators B hV wavelength sel
                  domain key small priorWave priorLeader with
              | error message =>
                  rw [hSmall] at hRun
                  contradiction
              | ok priorEpochs =>
                  have hLarge := ih (large := large) (wave := priorWave)
                    (leader := priorLeader) (epochs := priorEpochs) (by omega) hSmall
                  rw [hSmall] at hRun
                  rw [hLarge]
                  exact hRun

/-- Following a semantic previous-final-leader chain extends the earlier
recursive epoch plan. -/
theorem tauPlanFromLeader_prefix_of_chain
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (key : BlockId → String)
    (oldWave : ℕ) (oldLeader : BlockId)
    {steps newWave : ℕ} {newLeader : BlockId}
    (hChain : PreviousFinalLeaderChain bonds validators B hV wavelength sel domain
      oldWave oldLeader steps newWave newLeader) :
    ∀ {oldFuel newFuel oldEpochs newEpochs},
      oldFuel + steps ≤ newFuel →
      tauPlanFromLeader bonds validators B hV wavelength sel domain key
        oldFuel oldWave oldLeader = .ok oldEpochs →
      tauPlanFromLeader bonds validators B hV wavelength sel domain key
        newFuel newWave newLeader = .ok newEpochs →
      List.IsPrefix oldEpochs newEpochs := by
  induction hChain with
  | refl =>
      intro oldFuel newFuel oldEpochs newEpochs hFuel hOld hNew
      have hSame := tauPlanFromLeader_fuel_mono bonds validators B hV wavelength
        sel domain key hFuel hOld
      rw [hSame] at hNew
      have hEq := Except.ok.inj hNew
      subst newEpochs
      exact List.prefix_refl _
  | @step steps wave leader priorWave priorLeader hPrevious _ ih =>
      intro oldFuel newFuel oldEpochs newEpochs hFuel hOld hNew
      cases newFuel with
      | zero => simp [tauPlanFromLeader] at hNew
      | succ remaining =>
          cases hPrior : tauPlanFromLeader bonds validators B hV wavelength sel
              domain key remaining priorWave priorLeader with
          | error message =>
              have hComputed : tauPlanFromLeader bonds validators B hV wavelength
                  sel domain key (remaining + 1) wave leader = .error message := by
                unfold tauPlanFromLeader
                simp only [hPrevious]
                rw [hPrior]
                change Except.bind (Except.error message) _ = _
                rfl
              rw [hComputed] at hNew
              contradiction
          | ok priorEpochs =>
              let approved := domain.filter fun block =>
                checkApproves B hV leader block &&
                  !(flattenEpochs priorEpochs).contains block
              cases hSuffix : xsortRef (blockParents B) key approved with
              | error message =>
                  have hComputed : tauPlanFromLeader bonds validators B hV wavelength
                      sel domain key (remaining + 1) wave leader = .error message := by
                    unfold tauPlanFromLeader
                    simp only [hPrevious]
                    rw [hPrior]
                    change Except.map _ (xsortRef (blockParents B) key approved) = _
                    rw [hSuffix]
                    rfl
                  rw [hComputed] at hNew
                  contradiction
              | ok suffix =>
                  have hComputed : tauPlanFromLeader bonds validators B hV wavelength
                      sel domain key (remaining + 1) wave leader =
                        .ok (priorEpochs ++ [{ leader := leader, blocks := suffix }]) := by
                    unfold tauPlanFromLeader
                    simp only [hPrevious]
                    rw [hPrior]
                    change Except.map _ (xsortRef (blockParents B) key approved) = _
                    rw [hSuffix]
                    rfl
                  have hNewShape :
                      newEpochs = priorEpochs ++ [{ leader := leader, blocks := suffix }] := by
                    rw [hComputed] at hNew
                    exact (Except.ok.inj hNew).symm
                  subst newEpochs
                  have hPrefix := ih (oldFuel := oldFuel) (newFuel := remaining)
                    (oldEpochs := oldEpochs) (newEpochs := priorEpochs) (by omega)
                    hOld hPrior
                  exact hPrefix.trans
                    ⟨[{ leader := leader, blocks := suffix }], rfl⟩

/-- A successful plan with no latest final leader has no epochs. -/
theorem computeTauPlan_none_epochs
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (key : BlockId → String) (throughWave : ℕ)
    (epochs : List TauEpoch)
    (hPlan : computeTauPlan bonds validators B hV wavelength sel domain key
      throughWave = .ok (none, epochs)) : epochs = [] := by
  unfold computeTauPlan at hPlan
  cases hLatest : (List.range (throughWave + 1)).reverse.findSome?
      (finalLeaderAt? bonds validators B hV wavelength sel domain) with
  | none =>
      simp only [hLatest] at hPlan
      have hPair := Except.ok.inj hPlan
      exact (Prod.mk.inj hPair).2.symm
  | some leader =>
      simp only [hLatest] at hPlan
      cases hRun : tauPlanFromLeader bonds validators B hV wavelength sel domain key
          (throughWave + 2) (waveOfRound (blockDepth B hV leader) wavelength)
          leader <;> simp [hRun] at hPlan

/-- `computeTauPlan` exposes the recursive run rooted at its latest leader. -/
theorem computeTauPlan_some_run
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (key : BlockId → String) (throughWave : ℕ)
    (leader : BlockId) (epochs : List TauEpoch)
    (hPlan : computeTauPlan bonds validators B hV wavelength sel domain key
      throughWave = .ok (some leader, epochs)) :
    tauPlanFromLeader bonds validators B hV wavelength sel domain key
      (throughWave + 2) (waveOfRound (blockDepth B hV leader) wavelength) leader =
        .ok epochs := by
  unfold computeTauPlan at hPlan
  cases hLatest : (List.range (throughWave + 1)).reverse.findSome?
      (finalLeaderAt? bonds validators B hV wavelength sel domain) with
  | none =>
      simp only [hLatest] at hPlan
      have hPair := Except.ok.inj hPlan
      simp at hPair
  | some latest =>
      simp only [hLatest] at hPlan
      cases hRun : tauPlanFromLeader bonds validators B hV wavelength sel domain key
          (throughWave + 2) (waveOfRound (blockDepth B hV latest) wavelength)
          latest with
      | error message => simp [hRun] at hPlan
      | ok result =>
          simp only [hRun] at hPlan
          have hPair := Except.ok.inj hPlan
          have hLeader : latest = leader := Option.some.inj (Prod.mk.inj hPair).1
          have hEpochs : result = epochs := (Prod.mk.inj hPair).2
          subst latest
          simpa [hEpochs] using hRun

/-- Semantic conditions under which append-only blocklace growth preserves a
previously emitted τ prefix.

The shared indices make bonds, validators, wavelength, leader selection, and
the canonical sort key immutable.  The remaining fields concern protocol
observations: append-only growth, monotone replay inputs, stable earlier
finality/ratification, and a previous-final-leader chain from the new latest
leader back to the old one. Approved-fragment stability is derived from the
blocklace and domain growth fields. No field assumes an epoch or output prefix. -/
structure ValidBlocklaceGrowthEvidence
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain domain' : List BlockId) (key : BlockId → String)
    (throughWave throughWave' : ℕ) where
  subBlocklace : SubBlocklace B B'
  domainGrowth : AppendOnlyDomain B B' domain domain'
  horizonExtension : throughWave ≤ throughWave'
  stableFinality : StableEarlierFinality bonds validators B B' hV hV'
    wavelength sel domain domain' throughWave
  stableRatification : StableEarlierRatification bonds validators B B' hV hV'
    domain
  oldLatest : Option BlockId
  newLatest : Option BlockId
  oldEpochs : List TauEpoch
  newEpochs : List TauEpoch
  oldPlan : computeTauPlan bonds validators B hV wavelength sel domain key
    throughWave = .ok (oldLatest, oldEpochs)
  newPlan : computeTauPlan bonds validators B' hV' wavelength sel domain' key
    throughWave' = .ok (newLatest, newEpochs)
  oldLatestInDomain : ∀ leader, oldLatest = some leader → leader ∈ domain
  oldLatestWaveBound : ∀ leader, oldLatest = some leader →
    waveOfRound (blockDepth B hV leader) wavelength ≤ throughWave
  latestExtension : oldLatest = none ∨
    ∃ oldLeader newLeader steps,
      oldLatest = some oldLeader ∧ newLatest = some newLeader ∧
      steps + throughWave ≤ throughWave' ∧
      PreviousFinalLeaderChain bonds validators B' hV' wavelength sel domain'
        (waveOfRound (blockDepth B hV oldLeader) wavelength) oldLeader steps
        (waveOfRound (blockDepth B' hV' newLeader) wavelength) newLeader
  newOutputChecked : checkTopological (blockParents B')
    (flattenEpochs newEpochs) (flattenEpochs newEpochs) = true

/-- Backward-compatible name for the strengthened growth evidence. -/
abbrev ValidAppendEvidence := ValidBlocklaceGrowthEvidence

/-- Proposition-level valid-growth predicate. The witness is hidden so callers
state a protocol condition, not a particular internal plan representation. -/
def ValidBlocklaceGrowth
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain domain' : List BlockId) (key : BlockId → String)
    (throughWave throughWave' : ℕ) : Prop :=
  Nonempty (ValidBlocklaceGrowthEvidence bonds validators B B' hV hV' wavelength sel
    domain domain' key throughWave throughWave')

/-- Compatibility alias used by the original Issue #244 theorem. -/
def ValidAppend
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain domain' : List BlockId) (key : BlockId → String)
    (throughWave throughWave' : ℕ) : Prop :=
  ValidBlocklaceGrowth bonds validators B B' hV hV' wavelength sel
    domain domain' key throughWave throughWave'

theorem flattenEpochs_append (left right : List TauEpoch) :
    flattenEpochs (left ++ right) = flattenEpochs left ++ flattenEpochs right := by
  simp [flattenEpochs, List.flatMap_append]

/-- Flattening an epoch-prefix produces an output-prefix. -/
theorem flattenEpochs_prefix {left right : List TauEpoch}
    (h : List.IsPrefix left right) :
    List.IsPrefix (flattenEpochs left) (flattenEpochs right) := by
  rcases h with ⟨suffix, rfl⟩
  exact ⟨flattenEpochs suffix, (flattenEpochs_append left suffix).symm⟩

/-- A checked plan is exactly the successful `tauRef` output. -/
theorem tauRef_eq_of_plan
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B : Blocklace) (hV : ValidBlocklace B)
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain : List BlockId) (key : BlockId → String) (throughWave : ℕ)
    (latest : Option BlockId) (epochs : List TauEpoch)
    (hPlan : computeTauPlan bonds validators B hV wavelength sel domain key
      throughWave = .ok (latest, epochs))
    (hChecked : checkTopological (blockParents B)
      (flattenEpochs epochs) (flattenEpochs epochs) = true) :
    tauRef bonds validators B hV wavelength sel domain key throughWave =
      .ok (flattenEpochs epochs) := by
  unfold tauRef
  rw [hPlan]
  change (if checkTopological (blockParents B) (flattenEpochs epochs)
      (flattenEpochs epochs) = true then
    Except.ok (flattenEpochs epochs) else
    Except.error "tau output failed duplicate/predecessor validation") = _
  rw [hChecked]
  rfl

/-- **τ prefix theorem.** A valid append preserves the old output and only
adds a suffix. The hypotheses explicitly include append-only blocklace
growth, bounded earlier finality, unchanged ordering parameters, and semantic
continuation of the previous-final-leader chain. -/
theorem tauRef_prefix_of_validAppend
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain domain' : List BlockId) (key : BlockId → String)
    (throughWave throughWave' : ℕ)
    (hAppend : ValidAppend bonds validators B B' hV hV' wavelength sel
      domain domain' key throughWave throughWave')
    (oldOutput : List BlockId)
    (hOld : tauRef bonds validators B hV wavelength sel domain key throughWave =
      .ok oldOutput) :
    ∃ suffix,
      tauRef bonds validators B' hV' wavelength sel domain' key throughWave' =
        .ok (oldOutput ++ suffix) := by
  rcases hAppend with ⟨append⟩
  have hOldShape : oldOutput = flattenEpochs append.oldEpochs := by
    unfold tauRef at hOld
    rw [append.oldPlan] at hOld
    change (if checkTopological (blockParents B) (flattenEpochs append.oldEpochs)
        (flattenEpochs append.oldEpochs) = true then
      Except.ok (flattenEpochs append.oldEpochs) else
      Except.error "tau output failed duplicate/predecessor validation") =
        Except.ok oldOutput at hOld
    split at hOld
    · exact (Except.ok.inj hOld).symm
    · simp at hOld
  have hEpochs : List.IsPrefix append.oldEpochs append.newEpochs := by
    rcases append.latestExtension with hNoLeader | hLeaders
    · have hEmpty := computeTauPlan_none_epochs bonds validators B hV wavelength
        sel domain key throughWave append.oldEpochs (hNoLeader ▸ append.oldPlan)
      rw [hEmpty]
      exact List.nil_prefix
    · rcases hLeaders with
        ⟨oldLeader, newLeader, steps, hOldLatest, hNewLatest, hFits, hChain⟩
      have hOldRun := computeTauPlan_some_run bonds validators B hV wavelength sel
        domain key throughWave oldLeader append.oldEpochs (hOldLatest ▸ append.oldPlan)
      have hStable := tauPlanFromLeader_stable bonds validators B B' hV hV'
        wavelength sel domain domain' key throughWave append.subBlocklace
        append.domainGrowth append.stableFinality append.stableRatification
        (throughWave + 2)
        (waveOfRound (blockDepth B hV oldLeader) wavelength) oldLeader
        (by
          have hBound := append.oldLatestWaveBound oldLeader hOldLatest
          omega)
        (append.oldLatestInDomain oldLeader hOldLatest)
      have hOldRun' :
          tauPlanFromLeader bonds validators B' hV' wavelength sel domain' key
              (throughWave + 2)
              (waveOfRound (blockDepth B hV oldLeader) wavelength) oldLeader =
            .ok append.oldEpochs := by
        rw [← hStable]
        exact hOldRun
      have hNewRun := computeTauPlan_some_run bonds validators B' hV' wavelength sel
        domain' key throughWave' newLeader append.newEpochs
        (hNewLatest ▸ append.newPlan)
      exact tauPlanFromLeader_prefix_of_chain bonds validators B' hV' wavelength
        sel domain' key (waveOfRound (blockDepth B hV oldLeader) wavelength)
        oldLeader hChain (by omega) hOldRun' hNewRun
  rcases hEpochs with ⟨laterEpochs, hEpochs⟩
  subst oldOutput
  refine ⟨flattenEpochs laterEpochs, ?_⟩
  have hNew := tauRef_eq_of_plan bonds validators B' hV' wavelength sel domain'
    key throughWave' append.newLatest append.newEpochs append.newPlan
    append.newOutputChecked
  rw [← hEpochs, flattenEpochs_append] at hNew
  exact hNew

/-- Preferred spelling of the prefix theorem for semantic blocklace growth. -/
theorem tauRef_prefix_of_validBlocklaceGrowth
    (bonds : NodeId → ℕ) (validators : Finset NodeId)
    (B B' : Blocklace) (hV : ValidBlocklace B) (hV' : ValidBlocklace B')
    (wavelength : ℕ) (sel : ℕ → Option NodeId)
    (domain domain' : List BlockId) (key : BlockId → String)
    (throughWave throughWave' : ℕ)
    (hGrowth : ValidBlocklaceGrowth bonds validators B B' hV hV' wavelength sel
      domain domain' key throughWave throughWave')
    (oldOutput : List BlockId)
    (hOld : tauRef bonds validators B hV wavelength sel domain key throughWave =
      .ok oldOutput) :
    ∃ suffix,
      tauRef bonds validators B' hV' wavelength sel domain' key throughWave' =
        .ok (oldOutput ++ suffix) :=
  tauRef_prefix_of_validAppend bonds validators B B' hV hV' wavelength sel
    domain domain' key throughWave throughWave' hGrowth oldOutput hOld

/-! ## Small executable regression examples -/

private def noParents : ParentMap := fun _ => some []

private def chainParents : ParentMap
  | 0 => some []
  | 1 => some [0]
  | _ => none

private def cyclicParents : ParentMap
  | 0 => some [1]
  | 1 => some [0]
  | _ => none

/-- Canonical-key tie breaking is deterministic for simultaneously ready
vertices. -/
example : xsortRef noParents toString [2, 1] = .ok [1, 2] := by
  native_decide

/-- A predecessor is emitted before its child even when the input is reversed. -/
example : xsortRef chainParents toString [1, 0] = .ok [0, 1] := by
  native_decide

/-- Kahn's algorithm rejects a two-vertex cycle. -/
example : xsortRef cyclicParents toString [0, 1] =
    .error "tau subset contains a cycle or unknown block" := by
  native_decide

/-! The remaining examples construct an actual valid two-wave blocklace. With
one validator of weight one, each chain block is the selected final leader for
its wave; the second leader ratifies the first. -/

namespace TwoWaveExample

def validator : NodeId := 10

def content0 : BlockContent := { payload := [0], predecessors := ∅ }
def block0 : Block :=
  { id := hashContent validator content0, creator := validator,
    content := content0, id_eq := rfl }

def content1 : BlockContent := { payload := [1], predecessors := {block0.id} }
def block1 : Block :=
  { id := hashContent validator content1, creator := validator,
    content := content1, id_eq := rfl }

def B0 : Blocklace := emptyBlocklace
def B1 : Blocklace := blocklaceInsert B0 block0
def B2 : Blocklace := blocklaceInsert B1 block1

theorem block0_ne_block1 : block0.id ≠ block1.id := by
  intro h
  have hContent := (hashInj h).2
  simp [block0, content0, content1] at hContent

theorem B0_valid : ValidBlocklace B0 := ValidBlocklace.empty

theorem B1_valid : ValidBlocklace B1 := by
  apply ValidBlocklace.insert B0 block0 B0_valid
  · intro predecessor hPredecessor
    simp [block0, content0] at hPredecessor
  · intro hMember
    have h := Finmap.mem_keys.mp hMember
    simp [B0, emptyBlocklace, Finmap.mem_def] at h

theorem block0_mem_B1 : block0.id ∈ B1.keys := by
  exact Finmap.mem_keys.mpr (Finmap.mem_insert.mpr (Or.inl rfl))

theorem B2_valid : ValidBlocklace B2 := by
  apply ValidBlocklace.insert B1 block1 B1_valid
  · intro predecessor hPredecessor
    have hEq : predecessor = block0.id := by
      simpa [block1, content1] using hPredecessor
    simpa [hEq] using block0_mem_B1
  · intro hMember
    rcases Finmap.mem_insert.mp (Finmap.mem_keys.mp hMember) with hEq | hOld
    · exact block0_ne_block1 hEq.symm
    · have hEmpty : block1.id ∈ B0.keys := Finmap.mem_keys.mpr hOld
      have h := Finmap.mem_keys.mp hEmpty
      simp [B0, emptyBlocklace, Finmap.mem_def] at h

def bonds (node : NodeId) : ℕ := if node = validator then 1 else 0
def validators : Finset NodeId := {validator}
def selectLeader (_wave : ℕ) : Option NodeId := some validator
def key (block : BlockId) : String := toString block

/-- The executable model follows both final leaders and returns the expected
two-wave τ order. -/
example : tauRef bonds validators B2 B2_valid 1 selectLeader
    [block0.id, block1.id] key 1 = .ok [block0.id, block1.id] := by
  native_decide

/-- Extending the replay horizon from the first stable epoch to the second
preserves the first output as a prefix. -/
example : List.IsPrefix [block0.id]
    (match tauRef bonds validators B2 B2_valid 1 selectLeader
      [block0.id, block1.id] key 1 with
    | .ok output => output
    | .error _ => []) := by
  native_decide

end TwoWaveExample

end CordialMiners
