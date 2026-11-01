import { acceptContract, type ContractResult } from './contract';
import type { BoundResult } from './types';
export type PresentationValidation = ContractResult;

/** Native generation/persistence and the browser execute the same Rust
 * acceptance code. The returned spec has already been compiled to json-render. */
export function validatePresentation(input: unknown, results: BoundResult[]): PresentationValidation {
 try { return acceptContract(input, results); }
 catch(error) {return {valid:false,spec:null,issues:[{path:'root',message:error instanceof Error?error.message:'Presentation validation failed.'}],resultIds:[]};}
}
