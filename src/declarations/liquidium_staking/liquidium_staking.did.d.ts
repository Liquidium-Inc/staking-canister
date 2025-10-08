import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface FingerprintInfo { 'fingerprint' : string }
export interface PoolInfo {
  'xpub' : string,
  'address' : string,
  'fingerprint' : string,
  'index' : number,
}
export type Result = { 'Ok' : number } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : [[] | [bigint], [] | [bigint]] } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : string } |
  { 'Err' : string };
export interface TxRecord {
  'sliq_amount' : bigint,
  'liq_amount' : bigint,
  'txid' : string,
  'tx_type' : TxTypeEnum,
}
export type TxTypeEnum = { 'Stake' : null } |
  { 'Reward' : null } |
  { 'Unstake' : null };
export interface UnstakeRecord {
  'utxo' : string,
  'user_address' : string,
  'timestamp' : bigint,
  'rune_amount' : bigint,
}
export interface _SERVICE {
  'get_exchange_rate' : ActorMethod<[], Result>,
  'get_exchange_rate_components' : ActorMethod<[], Result_1>,
  'get_fingerprint' : ActorMethod<[[] | [number]], FingerprintInfo>,
  'get_pool' : ActorMethod<[[] | [number]], PoolInfo>,
  'get_pool_address' : ActorMethod<[[] | [number]], string>,
  'get_processing' : ActorMethod<[], Array<string>>,
  'get_recent_unstake_records' : ActorMethod<[], Array<UnstakeRecord>>,
  'get_recorded' : ActorMethod<[], Array<TxRecord>>,
  'get_tx' : ActorMethod<[string], Result_2>,
  'get_user_latest_unstake_record' : ActorMethod<
    [string],
    [] | [UnstakeRecord]
  >,
  'get_xpub' : ActorMethod<[[] | [number]], string>,
  'initialize_pool_addresses_range' : ActorMethod<[number, number], Result_2>,
  'insert_record' : ActorMethod<[TxRecord], undefined>,
  'parse_psbt_runes' : ActorMethod<[string], string>,
  'remove_processing' : ActorMethod<[string], undefined>,
  'reset_exchange_rate' : ActorMethod<[], undefined>,
  'stake' : ActorMethod<[string], string>,
  'start_cron' : ActorMethod<[], undefined>,
  'unstake' : ActorMethod<[string], string>,
  'withdraw' : ActorMethod<[string], string>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
