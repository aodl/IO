import { Actor, HttpAgent } from "@dfinity/agent";
import { idlFactory } from "../../declarations/io_historian/io_historian.did.js";
import { idlFactory as streamIdl } from "../../declarations/io_stream_manager/io_stream_manager.did.js";
import { idlFactory as ledgerIdl } from "../../declarations/io_ledger/io_ledger.did.js";

export function hostForNetwork(network) {
  if (network === "ic" || network === "mainnet") {
    return "https://icp0.io";
  }
  return undefined;
}

export function createRedemptionActors(config, identity, deps = {}) {
  if (!identity || !config.streamManagerCanisterId || !config.ioLedgerCanisterId) return null;
  const AgentCtor = deps.HttpAgent ?? HttpAgent;
  const ActorApi = deps.Actor ?? Actor;
  const agent = new AgentCtor({ identity, host: hostForNetwork(config.network) });
  const stream = ActorApi.createActor(streamIdl, {
    agent,
    canisterId: config.streamManagerCanisterId,
  });
  return {
    stream,
    ledger: ActorApi.createActor(ledgerIdl, { agent, canisterId: config.ioLedgerCanisterId }),
    streamCanister: ActorApi.canisterIdOf(stream),
  };
}

export function createHistorianActor(config, deps = {}) {
  if (!config.historianCanisterId) {
    return null;
  }
  const AgentCtor = deps.HttpAgent ?? HttpAgent;
  const ActorApi = deps.Actor ?? Actor;
  const agent = new AgentCtor({ host: hostForNetwork(config.network) });
  return ActorApi.createActor(idlFactory, {
    agent,
    canisterId: config.historianCanisterId,
  });
}
