# Testflight Proposal Checklist

Manual/mainnet only. Not CI. Not a real launch. No real swap.

- [ ] `sns_init.testflight.template.yaml` copied and filled with reviewed testflight values.
- [ ] Final tokenomics placeholders reviewed but not treated as production decisions.
- [ ] Fallback controllers defined and reviewed.
- [ ] Dapp canisters listed: `io_stream_manager`, `io_nns_neuron_manager`, `io_historian`, `frontend`.
- [ ] Release artifact hashes checked against `release-artifacts/manifest.json`.
- [ ] SNS root co-controller step planned for each dapp canister.
- [ ] Recovery controller path documented before testflight.
- [ ] Frontend canister configured to use the testflight historian canister ID.
- [ ] Historian read model checked after testflight upgrade proposal.
- [ ] SNS root controls intended dapp canisters after registration.
- [ ] Upgrade proposal tested through SNS governance.
- [ ] NNS Manager execution canister `oae4c-3iaaa-aaaar-qb5qq-cai` and
      protected IO NNS neuron `10292412127977304661` remain untouched.
