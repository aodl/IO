import { Actor, HttpAgent } from "@dfinity/agent";
import { idlFactory } from "../../declarations/io_historian/io_historian.did.js";

export function hostForNetwork(network) {
  if (network === "ic" || network === "mainnet") {
    return "https://icp0.io";
  }
  return undefined;
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
