//! Additive maintained-lifecycle proof using the single acceptance fixture.
use super::*;
use io_historian::{Dashboard, ModuleMatch, ObservationFreshness};
use sha2::{Digest, Sha256};

#[derive(CandidType)]
struct CanisterStatusArgs {
    canister_id: Principal,
}

#[derive(CandidType, Deserialize)]
struct LedgerLength {
    log_length: Nat,
}

impl Sale {
    fn context(&self) -> Value {
        serde_json::from_slice(&fs::read(self.evidence.join("context.json")).unwrap()).unwrap()
    }

    fn role(&self, role: &str) -> Principal {
        Principal::from_text(self.context()[role].as_str().unwrap()).unwrap()
    }

    fn nns_status(&self, label: &str) -> io_nns_neuron_manager::Status {
        decode_one(&self.raw_query(
            self.role("nns_manager"),
            "get_status",
            candid::encode_args(()).unwrap(),
            label,
        ))
        .unwrap()
    }

    pub(super) fn signed_local_ingress(&mut self) {
        fs::write(
            self.evidence.join("fresh-topology.txt"),
            format!(
                "instance_id={}\n{:#?}\n",
                self.pic.instance_id(),
                self.pic.topology()
            ),
        )
        .unwrap();
        let cli = PathBuf::from(
            std::env::var_os("IO_ACCEPTANCE_ICP_CLI")
                .expect("provide the verified pinned icp 0.2.7 binary"),
        );
        assert_eq!(
            hex::encode(Sha256::digest(fs::read(&cli).unwrap())),
            "99aaef26bd765ce197c1de525ddb437ad1d3e933e5d3ca2d720ed189c23b7667"
        );
        let version = Command::new(&cli).arg("--version").output().unwrap();
        assert!(version.status.success());
        assert_eq!(
            String::from_utf8(version.stdout).unwrap().trim(),
            "icp 0.2.7"
        );

        // Never use the host's shared CLI home, keyring, or signing identities.
        // This sibling TempDir is outside the evidence inventory and removed
        // before returning. No key/seed is read, printed, or copied by this test.
        let identity = tempfile::Builder::new()
            .prefix("io-ephemeral-signing-")
            .tempdir_in(self.evidence.parent().unwrap())
            .unwrap();
        let command = || {
            let mut cmd = Command::new(&cli);
            cmd.env("ICP_HOME", identity.path())
                .current_dir(identity.path());
            cmd
        };
        let created = command()
            .args([
                "identity",
                "new",
                "maintained-local",
                "--storage",
                "plaintext",
                "--output-seed",
            ])
            .arg(identity.path().join("local-recovery-material"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(created.success(), "isolated local identity creation failed");
        let principal = command()
            .args(["identity", "principal", "--identity", "maintained-local"])
            .output()
            .unwrap();
        assert!(principal.status.success());
        let sender =
            Principal::from_text(String::from_utf8(principal.stdout).unwrap().trim()).unwrap();
        assert_ne!(sender, self.operator);
        assert_ne!(sender, Principal::anonymous());
        let recipient = Principal::from_slice(&[104; 29]);
        self.transfer_icp(
            Principal::anonymous(),
            Account::new(sender, None),
            200_000_000,
        );
        let ledger = self.icp;
        let balance = |pic: &pocket_ic::PocketIc, owner| -> u64 {
            let value: IcpTokens = icrc::query_one(
                pic,
                ledger,
                "account_balance",
                IcpBalance {
                    account: Account::new(owner, None)
                        .icp_account_identifier_bytes()
                        .to_vec(),
                },
            );
            value.e8s
        };
        assert_eq!(balance(&self.pic, sender), 200_000_000);
        assert_eq!(balance(&self.pic, recipient), 0);
        let args = encode_one(IcpTransferArgs {
            memo: 0x494f_494e_4752_4553,
            amount: IcpTokens { e8s: 100_000_000 },
            fee: IcpTokens { e8s: 10_000 },
            from_subaccount: None,
            to: Account::new(recipient, None)
                .icp_account_identifier_bytes()
                .to_vec(),
            created_at_time: None,
        })
        .unwrap();
        fs::write(self.evidence.join("signed-ingress-args.bin"), &args).unwrap();
        let gateway = self.pic.make_live(None);
        assert!(matches!(
            gateway.host_str(),
            Some("127.0.0.1" | "localhost")
        ));
        let root_key = hex::encode(self.pic.root_key().unwrap());
        let host_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let simulated_nanos = u128::from(self.pic.get_time().as_nanos_since_unix_epoch());
        assert!(
            host_nanos.abs_diff(simulated_nanos) < 300_000_000_000,
            "signed ingress must precede prepare_permanent's first large time advance"
        );
        let result = command()
            .args([
                "canister",
                "call",
                "--network",
                gateway.as_str(),
                "--root-key",
                &root_key,
                "--identity",
                "maintained-local",
                &self.icp.to_text(),
                "transfer",
                &hex::encode(&args),
                "--args-format",
                "hex",
                "--output",
                "hex",
            ])
            .output()
            .unwrap();
        self.pic.stop_live();
        fs::write(
            self.evidence.join("signed-ingress-response.hex"),
            &result.stdout,
        )
        .unwrap();
        fs::write(
            self.evidence.join("signed-ingress-stderr.txt"),
            &result.stderr,
        )
        .unwrap();
        assert!(
            result.status.success(),
            "signed local transfer: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let reply = hex::decode(String::from_utf8(result.stdout).unwrap().trim()).unwrap();
        fs::write(self.evidence.join("signed-ingress-response.bin"), &reply).unwrap();
        let block = decode_one::<Result<u64, IcpTransferError>>(&reply)
            .unwrap()
            .unwrap();
        assert_eq!(balance(&self.pic, sender), 99_990_000);
        assert_eq!(balance(&self.pic, recipient), 100_000_000);
        let tip: LedgerTip = decode_one(
            &self.raw_query(
                self.icp,
                "query_blocks",
                encode_one(LedgerTipRequest {
                    start: block,
                    length: 1,
                })
                .unwrap(),
                "signed-ingress-ledger-block",
            ),
        )
        .unwrap();
        assert!(tip.chain_length > block);
        write_json(
            &self.evidence.join("signed-ingress.json"),
            &json!({
                "cli_version":"icp 0.2.7", "gateway":gateway.as_str(), "instance_id":self.pic.instance_id(),
                "sender":sender.to_text(), "recipient":recipient.to_text(), "ledger":self.icp.to_text(),
                "block_index":block, "amount_e8s":100_000_000, "fee_e8s":10_000,
            "sender_before_e8s":200_000_000, "sender_after_e8s":balance(&self.pic,sender),
            "recipient_before_e8s":0, "recipient_after_e8s":balance(&self.pic,recipient),
                "host_nanos":host_nanos.to_string(), "simulated_nanos":simulated_nanos.to_string(),
                "gateway_stopped_before_first_large_advance":true, "fixed_operator_cryptographically_authenticated":false,
                "signing_material_included":false, "protocol_funding_or_backing_e8s":0
            }),
        );
        identity.close().unwrap();
    }

    fn observed_version(&self, target: Principal, label: &str) -> pocket_ic::CanisterStatusResult {
        let raw = self
            .pic
            .update_call_with_effective_principal(
                Principal::management_canister(),
                pocket_ic::common::rest::RawEffectivePrincipal::CanisterId(
                    target.as_slice().to_vec(),
                ),
                self.root,
                "canister_status",
                encode_one(CanisterStatusArgs {
                    canister_id: target,
                })
                .unwrap(),
            )
            .unwrap();
        fs::write(
            self.evidence.join(format!("{label}-management-status.bin")),
            &raw,
        )
        .unwrap();
        decode_one(&raw).unwrap()
    }

    fn dynamic_observation(&self, label: &str) -> io_nns_neuron_manager::api::DynamicBackingStatus {
        // This existing public observation only reads canonical assets/policy;
        // it neither resumes monetary work nor acts as a keeper.
        let raw = self
            .pic
            .update_call(
                self.role("nns_manager"),
                Principal::anonymous(),
                "observe_dynamic_backing_status",
                candid::encode_args(()).unwrap(),
            )
            .unwrap();
        fs::write(self.evidence.join(format!("{label}-dynamic.bin")), &raw).unwrap();
        let observed = decode_one::<
            Result<
                io_nns_neuron_manager::api::DynamicBackingStatus,
                io_nns_neuron_manager::ApiError,
            >,
        >(&raw)
        .unwrap()
        .unwrap();
        fs::write(
            self.evidence.join(format!("{label}-dynamic.txt")),
            format!("{observed:#?}\n"),
        )
        .unwrap();
        let parent = observed.parent.as_ref().unwrap();
        assert_eq!(
            parent.staking_account.owner,
            self.historian_config().nns_governance
        );
        assert_eq!(
            parent.staking_account.subaccount,
            Some(gov::compute_neuron_staking_subaccount(self.role("nns_manager"), 0).to_vec())
        );
        assert_eq!(observed.anchor_target_e8s, 1_000_000_000);
        assert_eq!(
            parent.physical_principal_e8s,
            observed.claim_bearing_dynamic_principal_e8s
                + observed.anchor_available_e8s
                + observed.excluded_dynamic_surplus_e8s
        );
        assert_eq!(
            observed.claim_bearing_dynamic_principal_e8s,
            self.stream_status()
                .latest_reconciliation_checkpoint
                .unwrap()
                .pooled_backing_e8s
        );
        observed
    }

    fn sns_restart(&self, role: &str, arg: Vec<u8>) {
        let target = self.role(role);
        let modules: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("io-artifacts.json")).unwrap())
                .unwrap();
        let chunks: Value = serde_json::from_slice(
            &fs::read(self.evidence.join("preuploaded-chunks.json")).unwrap(),
        )
        .unwrap();
        let expected = hex::decode(modules[role]["raw_wasm_sha256"].as_str().unwrap()).unwrap();
        let before = self.observed_version(target, &format!("restart-{role}-before"));
        assert_eq!(before.module_hash.as_ref(), Some(&expected));
        assert_eq!(before.settings.controllers, vec![self.root]);
        let fees = self.neuron(self.operator, OPERATOR_MEMO).neuron_fees_e8s;
        let id = proposal_id(
            self.propose(
                &format!("Maintained same-source {role} restart"),
                gov::Action::UpgradeSnsControlledCanister(gov::UpgradeSnsControlledCanister {
                    canister_id: Some(target),
                    mode: Some(3),
                    new_canister_wasm: vec![],
                    canister_upgrade_arg: Some(arg),
                    chunked_canister_wasm: Some(gov::ChunkedCanisterWasm {
                        wasm_module_hash: expected.clone(),
                        store_canister_id: Some(target),
                        chunk_hashes_list: chunks[role]["chunk_hashes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| hex::decode(v.as_str().unwrap()).unwrap())
                            .collect(),
                    }),
                }),
            ),
        );
        let pending = self.proposal(id);
        assert_eq!(pending.decided_timestamp_seconds, 0);
        assert_eq!(pending.executed_timestamp_seconds, 0);
        assert_eq!(
            self.neuron(self.operator, OPERATOR_MEMO).neuron_fees_e8s,
            fees + 1_000_000_000
        );
        fs::write(
            self.evidence.join(format!("restart-{role}-pending.bin")),
            encode_one(pending).unwrap(),
        )
        .unwrap();
        unauthorized(self.manage(
            self.buyer,
            gov::compute_neuron_staking_subaccount(self.operator, JUPITER_MEMO).to_vec(),
            gov::Command::RegisterVote(gov::RegisterVote {
                proposal: Some(gov::ProposalId { id }),
                vote: 1,
            }),
        ));
        self.jupiter_vote(id, 1);
        let proposal = self.proposal(id);
        assert!(
            proposal.executed_timestamp_seconds > 0,
            "restart proposal: {proposal:?}"
        );
        fs::write(
            self.evidence.join(format!("restart-{role}-executed.bin")),
            encode_one(proposal).unwrap(),
        )
        .unwrap();
        let mut observed = None;
        for _ in 0..120 {
            let after = self.observed_version(target, &format!("restart-{role}-after"));
            // Root performs Stop request, Stop completion, Upgrade, Start.
            // Each increments the pinned IC canister version. Waiting for the
            // complete sequence avoids mistaking an early Stop for an upgrade.
            if after.version >= before.version + 4 && after.status == before.status {
                observed = Some(after);
                break;
            }
            self.pic.tick();
        }
        let after =
            observed.expect("Root acknowledged but did not perform the same-source upgrade");
        assert_eq!(after.version, before.version + 4);
        assert_eq!(after.status, before.status);
        assert_eq!(after.module_hash.as_ref(), Some(&expected));
        assert_eq!(after.settings.controllers, vec![self.root]);
        assert_eq!(
            self.neuron(self.operator, OPERATOR_MEMO).neuron_fees_e8s,
            fees
        );
        let voter = self.neuron(self.operator, JUPITER_MEMO);
        assert_eq!(
            voter.permissions,
            vec![gov::NeuronPermission {
                principal: Some(self.operator),
                permission_type: vec![4]
            }]
        );
        assert!(voter.followees.is_empty() && voter.topic_followees.is_none());
        write_json(
            &self.evidence.join(format!("restart-{role}.json")),
            &json!({
                "proposal_id":id,"target":target.to_text(),"mode":"Upgrade","sole_controller":self.root.to_text(),
                "before_canister_version":before.version,"after_canister_version":after.version,
                "before_module_hash":hex::encode(before.module_hash.unwrap()),"after_module_hash":hex::encode(expected),
                "proposal_fee_before_e8s":fees,"proposal_fee_after_e8s":fees,
                "operator_authorization":"PocketIC caller simulation; separate real ephemeral client ingress proof"
            }),
        );
    }

    fn historian_config(&self) -> io_historian::ObservationConfig {
        decode_one::<Option<io_historian::ObservationConfig>>(
            &fs::read(self.evidence.join("constructors/historian.bin")).unwrap(),
        )
        .unwrap()
        .unwrap()
    }

    fn historian(&self, label: &str) -> Dashboard {
        let id = self.role("historian");
        let dashboard: Dashboard = decode_one(&self.raw_query(
            id,
            "get_dashboard_state",
            candid::encode_args(()).unwrap(),
            &format!("{label}-dashboard"),
        ))
        .unwrap();
        let status: io_historian::PublicStatus = decode_one(&self.raw_query(
            id,
            "get_public_status",
            candid::encode_args(()).unwrap(),
            &format!("{label}-public-status"),
        ))
        .unwrap();
        let snapshot: io_historian::ProtocolSnapshot = decode_one(&self.raw_query(
            id,
            "get_protocol_snapshot",
            candid::encode_args(()).unwrap(),
            &format!("{label}-protocol"),
        ))
        .unwrap();
        let rate: Option<io_historian::ClaimRateSnapshot> = decode_one(&self.raw_query(
            id,
            "get_claim_rate",
            candid::encode_args(()).unwrap(),
            &format!("{label}-claim-rate"),
        ))
        .unwrap();
        assert_eq!(dashboard.status, status);
        assert_eq!(dashboard.protocol, snapshot);
        assert_eq!(rate, snapshot.claim_rate);
        fs::write(
            self.evidence.join(format!("{label}-dashboard.txt")),
            format!("{dashboard:#?}\n"),
        )
        .unwrap();
        dashboard
    }

    fn historian_converges(&self, label: &str) -> Dashboard {
        let config = self.historian_config();
        let mut last = self.historian(&format!("{label}-0"));
        for attempt in 0..180 {
            let canonical = self.stream_status();
            fs::write(
                self.evidence
                    .join(format!("{label}-{attempt}-canonical-stream.bin")),
                encode_one(&canonical).unwrap(),
            )
            .unwrap();
            // Freshness is age/availability metadata, not a promise that a
            // cached read model already contains the latest monetary event.
            let caught_up = canonical
                .latest_reconciliation_checkpoint
                .as_ref()
                .is_some_and(|checkpoint| {
                    canonical.operation_kind.is_none()
                        && last.stream.as_ref().is_some_and(|stream| {
                            stream
                                .latest_reconciliation_checkpoint
                                .as_ref()
                                .is_some_and(|projection| {
                                    projection.generation == checkpoint.generation
                                })
                                && stream.processed_reward_event_count
                                    == canonical.processed_reward_event_count
                                && stream.accumulated_eligible_credit
                                    == canonical.accumulated_eligible_credit
                                && stream.accumulated_policy_credit
                                    == canonical.accumulated_policy_credit
                        })
                        && last.protocol.active_reward_io_e8s
                            == Some(checkpoint.active_reward_io_e8s)
                        && last.protocol.active_backing_io_e8s
                            == Some(checkpoint.active_backing_io_e8s)
                        && last.protocol.total_claim_backing_e8s
                            == Some(checkpoint.total_claim_backing_e8s)
                });
            let length: LedgerLength = icrc::query_one(
                &self.pic,
                self.ledger,
                "get_transactions",
                icrc::LedgerGetTransactionsArgs {
                    start: Nat::from(0u64),
                    length: Nat::from(0u64),
                },
            );
            let indexed = last.index.as_ref().is_some_and(|index| {
                index.num_blocks_synced == u128::try_from(length.log_length.0.clone()).unwrap()
            });
            if last.status.configured
                && !last.status.refresh_active
                && last.source_health.len() == 7
                && last
                    .source_health
                    .iter()
                    .all(|h| h.freshness == ObservationFreshness::Fresh)
                && last.protocol.completeness.claim_rate
                && caught_up
                && indexed
            {
                self.reconcile_historian(&last, &config);
                return last;
            }
            self.pic
                .advance_time(Duration::from_secs(config.refresh_interval_seconds));
            for _ in 0..200 {
                self.pic.tick();
            }
            last = self.historian(&format!("{label}-{}", attempt + 1));
        }
        panic!("Historian did not converge autonomously: {last:#?}");
    }

    fn reconcile_historian(&self, dashboard: &Dashboard, config: &io_historian::ObservationConfig) {
        assert_eq!(config.stream_manager, self.stream);
        assert_eq!(config.nns_manager, self.role("nns_manager"));
        assert_eq!(config.sns_root, self.root);
        assert_eq!(config.sns_governance, self.governance);
        assert_eq!(config.sns_ledger, self.ledger);
        assert_eq!(config.sns_index, self.role("index"));
        assert_eq!(dashboard.canisters.len(), config.expected_modules.len());
        for expected in &config.expected_modules {
            let matches: Vec<_> = dashboard
                .canisters
                .iter()
                .filter(|c| c.canister_id == expected.canister_id)
                .collect();
            assert_eq!(matches.len(), 1);
            let observed = matches[0];
            assert_eq!(observed.role, expected.role);
            assert_eq!(observed.module_match, ModuleMatch::Matching);
            assert_eq!(
                observed.observed_module_hash.as_ref(),
                Some(&expected.wasm_sha256)
            );
            let controller = if expected.canister_id == self.root {
                self.governance
            } else {
                self.root
            };
            assert_eq!(observed.controllers, Some(vec![controller]));
            let physical = self
                .pic
                .canister_status(expected.canister_id, Some(controller))
                .unwrap();
            assert_eq!(physical.module_hash, observed.observed_module_hash);
            assert_eq!(
                physical.settings.controllers,
                observed.controllers.clone().unwrap()
            );
        }
        let total: Nat = icrc::query_one(&self.pic, self.ledger, "icrc1_total_supply", ());
        let reserve = self.balance(
            config.protocol_io_reserve.owner,
            config.protocol_io_reserve.subaccount.map(|s| s.0.to_vec()),
        );
        let excluded: u128 = config
            .nonredeemable_governance_io_accounts
            .iter()
            .map(|n| {
                u128::from(
                    self.balance(n.account.owner, n.account.subaccount.map(|s| s.0.to_vec())),
                )
            })
            .sum();
        assert_eq!(excluded, 10_000_000_000_000_000);
        let total = u128::try_from(total.0).unwrap();
        let protocol = &dashboard.protocol;
        assert_eq!(protocol.total_io_supply_e8s, Some(total));
        assert_eq!(protocol.protocol_reserve_io_e8s, Some(u128::from(reserve)));
        assert_eq!(protocol.nonredeemable_governance_io_e8s, Some(excluded));
        assert_eq!(
            protocol.claim_io_supply_e8s,
            Some(total - u128::from(reserve) - excluded)
        );
        let stream = self.stream_status();
        assert!(stream.operation_kind.is_none());
        let checkpoint = stream.latest_reconciliation_checkpoint.as_ref().unwrap();
        assert_eq!(
            protocol.claim_io_supply_e8s,
            Some(checkpoint.claim_supply_e8s)
        );
        assert_eq!(
            protocol.total_claim_backing_e8s,
            Some(checkpoint.total_claim_backing_e8s)
        );
        assert_eq!(
            protocol.pooled_parent_principal_e8s,
            Some(checkpoint.pooled_backing_e8s)
        );
        assert_eq!(
            protocol.live_child_net_backing_e8s,
            Some(checkpoint.unwinding_backing_e8s)
        );
        assert_eq!(
            protocol.in_transit_backing_e8s,
            Some(checkpoint.transit_backing_e8s)
        );
        assert_eq!(
            protocol.active_backing_io_e8s,
            Some(checkpoint.active_backing_io_e8s)
        );
        assert_eq!(
            protocol.active_reward_io_e8s,
            Some(checkpoint.active_reward_io_e8s)
        );
        let liquid: IcpTokens = icrc::query_one(
            &self.pic,
            self.icp,
            "account_balance",
            IcpBalance {
                account: config
                    .liquid_icp_reserve
                    .icp_account_identifier_bytes()
                    .to_vec(),
            },
        );
        assert_eq!(u128::from(liquid.e8s), checkpoint.liquid_backing_e8s);
        let dynamic: IcpTokens = icrc::query_one(
            &self.pic,
            self.icp,
            "account_balance",
            IcpBalance {
                account: Account::new(
                    config.nns_governance,
                    Some(Subaccount(gov::compute_neuron_staking_subaccount(
                        config.nns_manager,
                        0,
                    ))),
                )
                .icp_account_identifier_bytes()
                .to_vec(),
            },
        );
        assert!(u128::from(dynamic.e8s) >= checkpoint.pooled_backing_e8s);
        let permanent = self.nns_info(config.two_year_neuron_id);
        assert_eq!(
            protocol.permanent_productive_capital_e8s,
            Some(
                u128::from(permanent.stake_e8s)
                    + u128::from(permanent.staked_maturity_e8s_equivalent.unwrap_or(0))
            )
        );
        let rate = protocol.claim_rate.as_ref().unwrap();
        assert_eq!(
            rate.backing_numerator_e8s,
            checkpoint.total_claim_backing_e8s
        );
        assert_eq!(rate.claim_denominator_e8s, checkpoint.claim_supply_e8s);
        assert_eq!(rate.available_liquid_e8s, u128::from(liquid.e8s));
        let sns = dashboard.sns.as_ref().unwrap();
        assert_eq!(
            sns.reward_share_capability,
            io_historian::CapabilityState::ExpectedGovernanceModuleMatching
        );
        assert_eq!(sns.max_number_of_neurons, Some(1000));
        assert_eq!(sns.native_initial_reward_rate_basis_points, Some(0));
        assert_eq!(sns.native_final_reward_rate_basis_points, Some(0));
        let length: LedgerLength = icrc::query_one(
            &self.pic,
            self.ledger,
            "get_transactions",
            icrc::LedgerGetTransactionsArgs {
                start: Nat::from(0u64),
                length: Nat::from(0u64),
            },
        );
        assert_eq!(
            dashboard.index.as_ref().unwrap().num_blocks_synced,
            u128::try_from(length.log_length.0).unwrap()
        );
        assert!(
            dashboard.index.as_ref().unwrap().accounts.is_empty(),
            "canonical constructor configures no history Accounts"
        );
        let observed_stream = dashboard.stream.as_ref().unwrap();
        assert_eq!(
            observed_stream.processed_reward_event_count,
            stream.processed_reward_event_count
        );
        assert_eq!(
            observed_stream.accumulated_eligible_credit,
            stream.accumulated_eligible_credit
        );
        assert_eq!(
            observed_stream.accumulated_policy_credit,
            stream.accumulated_policy_credit
        );
    }
}

pub(super) fn run() {
    let sale = Sale::new_with_signed_ingress(true);
    sale.migrate_reserve();
    sale.capitalize();
    sale.install_io(false);
    let before = sale.historian_converges("maintained-before");
    assert_eq!(before.protocol.claim_io_supply_e8s, Some(3_000_000_000));
    assert_eq!(before.protocol.total_claim_backing_e8s, Some(3_000_000_000));
    let manager_before = sale.nns_status("maintained-manager-before");
    let dynamic_before = sale.dynamic_observation("maintained-before");
    let permanent_id = sale.historian_config().two_year_neuron_id;
    let permanent_before = sale.nns_info(permanent_id);
    let stream_before = sale.stream_status();
    sale.sns_restart("nns_manager", candid::encode_args(()).unwrap());
    let mut ready = false;
    for _ in 0..120 {
        let state = sale.nns_status("maintained-manager-after");
        if state.lifecycle == io_nns_neuron_manager::Lifecycle::Ready
            && state.active_operation.is_none()
        {
            assert_eq!(state, manager_before);
            ready = true;
            break;
        }
        sale.pic.advance_time(Duration::from_secs(60));
        for _ in 0..200 {
            sale.pic.tick();
        }
    }
    assert!(ready, "NNS Manager did not autonomously recover Ready");
    assert_eq!(
        sale.dynamic_observation("maintained-recovered"),
        dynamic_before
    );
    assert_eq!(
        sale.nns_info(permanent_id).stake_e8s,
        permanent_before.stake_e8s
    );
    assert_eq!(
        sale.stream_status().latest_reconciliation_checkpoint,
        stream_before.latest_reconciliation_checkpoint
    );
    let historian_before_restart = sale.historian_converges("maintained-before-historian-restart");
    sale.sns_restart(
        "historian",
        encode_one(None::<io_historian::ObservationConfig>).unwrap(),
    );
    let stale = sale.historian("maintained-after-restart");
    assert!(stale.status.configured);
    assert_eq!(
        stale.status.refresh_generation,
        historian_before_restart.status.refresh_generation
    );
    assert!(!stale.status.refresh_active);
    assert!(stale
        .source_health
        .iter()
        .all(|h| h.freshness == ObservationFreshness::Stale));
    let recovered = sale.historian_converges("maintained-recovered");
    assert!(
        recovered.status.refresh_generation > historian_before_restart.status.refresh_generation
    );
    assert_eq!(
        recovered.protocol.claim_io_supply_e8s,
        before.protocol.claim_io_supply_e8s
    );
    assert_eq!(
        recovered.protocol.total_claim_backing_e8s,
        before.protocol.total_claim_backing_e8s
    );
    sale.redemption_and_reward();
    sale.historian_converges("maintained-final");
    sale.dynamic_observation("maintained-final");
    write_json(
        &sale.evidence.join("maintained-result.json"),
        &json!({
            "result":"PASS","instance_id":sale.pic.instance_id(),"keeper_calls":0,
            "same_source_sns_controlled_upgrades":["nns_manager","historian"],
            "live_operator_signing_proved":false,"universal_liquidity_deadline_proved":false,
            "historical_sns_w_sale_executed":false
        }),
    );
    seal_evidence(&sale.evidence).unwrap();
    verify_evidence(&sale.evidence).unwrap();
    println!(
        "maintained sale-free lifecycle evidence: {}",
        sale.evidence.display()
    );
}

fn evidence_inventory(dir: &Path) -> Result<std::collections::BTreeMap<String, String>, String> {
    fn walk(
        base: &Path,
        dir: &Path,
        result: &mut std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err("evidence symlinks are forbidden".into());
            }
            if kind.is_dir() {
                walk(base, &path, result)?;
            } else if kind.is_file() {
                let name = path
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("non-UTF8 evidence path")?
                    .to_string();
                if name != "evidence-manifest.json" && name != "SHA256SUMS" {
                    result.insert(
                        name,
                        hex::encode(Sha256::digest(fs::read(path).map_err(|e| e.to_string())?)),
                    );
                }
            }
        }
        Ok(())
    }
    let mut result = std::collections::BTreeMap::new();
    walk(dir, dir, &mut result)?;
    Ok(result)
}

fn seal_evidence(dir: &Path) -> Result<(), String> {
    let files = evidence_inventory(dir)?;
    let manifest = serde_json::to_vec_pretty(
        &json!({"schema":"io.local.maintained-sale-free-evidence.v1","files":files}),
    )
    .map_err(|e| e.to_string())?;
    fs::write(dir.join("evidence-manifest.json"), &manifest).map_err(|e| e.to_string())?;
    let mut sums = files
        .iter()
        .map(|(name, hash)| format!("{hash}  {name}\n"))
        .collect::<String>();
    sums.push_str(&format!(
        "{}  evidence-manifest.json\n",
        hex::encode(Sha256::digest(&manifest))
    ));
    fs::write(dir.join("SHA256SUMS"), sums).map_err(|e| e.to_string())
}

fn verify_evidence(dir: &Path) -> Result<(), String> {
    let bytes = fs::read(dir.join("evidence-manifest.json")).map_err(|e| e.to_string())?;
    let manifest: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let expected: std::collections::BTreeMap<String, String> =
        serde_json::from_value(manifest["files"].clone()).map_err(|e| e.to_string())?;
    if expected != evidence_inventory(dir)? {
        return Err("evidence inventory/hash mismatch".into());
    }
    let mut sums = expected
        .iter()
        .map(|(name, hash)| format!("{hash}  {name}\n"))
        .collect::<String>();
    sums.push_str(&format!(
        "{}  evidence-manifest.json\n",
        hex::encode(Sha256::digest(bytes))
    ));
    if fs::read_to_string(dir.join("SHA256SUMS")).map_err(|e| e.to_string())? != sums {
        return Err("SHA256SUMS mismatch".into());
    }
    Ok(())
}

#[test]
fn evidence_inventory_rejects_missing_corrupt_and_extra_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("observed.bin");
    fs::write(&path, b"actual observed response").unwrap();
    seal_evidence(dir.path()).unwrap();
    verify_evidence(dir.path()).unwrap();
    fs::write(&path, b"changed").unwrap();
    assert!(verify_evidence(dir.path()).is_err());
    fs::remove_file(&path).unwrap();
    assert!(verify_evidence(dir.path()).is_err());
    fs::write(&path, b"actual observed response").unwrap();
    fs::write(dir.path().join("unrecorded.bin"), b"extra").unwrap();
    assert!(verify_evidence(dir.path()).is_err());
    fs::remove_file(dir.path().join("unrecorded.bin")).unwrap();
    verify_evidence(dir.path()).unwrap();
}
