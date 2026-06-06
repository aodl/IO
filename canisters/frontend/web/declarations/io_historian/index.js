import { Actor, HttpAgent } from "@dfinity/agent";
import { idlFactory } from "./io_historian.did.js";

export { idlFactory };

export function createActor(canisterId, options = {}) {
  const agent = options.agent ?? new HttpAgent({ host: options.host });
  return Actor.createActor(idlFactory, { agent, canisterId });
}
