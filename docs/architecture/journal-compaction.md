# Historian history retention

The simplified value-moving canisters do not use a generic operation journal. They persist one typed active stream operation, one typed immediate NNS operation, fixed pending slots and bounded exact replay evidence. Those records are never compacted away while active.

The historian independently owns observation histories, source watermarks and index/archive ingestion metadata. Historian retention may use bounded checkpoint-and-prune policies because historian state is not monetary authority. Pruning must preserve explicit source health, latest observation watermarks, audit provenance and any retained public-history guarantees.

Stable-storage validation treats this document as the retention boundary: compaction applies only to historian/read-model history, never to an active monetary intent, caller replay result, pending cohort, pending maturity or pending unwind.
