export const idlFactory = ({ IDL }) => {
  const Result = IDL.Variant({ 'Ok' : IDL.Float64, 'Err' : IDL.Text });
  const Result_1 = IDL.Variant({
    'Ok' : IDL.Tuple(IDL.Opt(IDL.Nat), IDL.Opt(IDL.Nat)),
    'Err' : IDL.Text,
  });
  const FingerprintInfo = IDL.Record({ 'fingerprint' : IDL.Text });
  const PoolInfo = IDL.Record({
    'xpub' : IDL.Text,
    'address' : IDL.Text,
    'fingerprint' : IDL.Text,
    'index' : IDL.Nat32,
  });
  const UnstakeRecord = IDL.Record({
    'utxo' : IDL.Text,
    'user_address' : IDL.Text,
    'timestamp' : IDL.Nat64,
    'rune_amount' : IDL.Nat,
  });
  const TxTypeEnum = IDL.Variant({
    'Stake' : IDL.Null,
    'Reward' : IDL.Null,
    'Unstake' : IDL.Null,
  });
  const TxRecord = IDL.Record({
    'sliq_amount' : IDL.Nat,
    'liq_amount' : IDL.Nat,
    'txid' : IDL.Text,
    'tx_type' : TxTypeEnum,
  });
  const Result_2 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  return IDL.Service({
    'get_exchange_rate' : IDL.Func([], [Result], ['query']),
    'get_exchange_rate_components' : IDL.Func([], [Result_1], ['query']),
    'get_fingerprint' : IDL.Func(
        [IDL.Opt(IDL.Nat32)],
        [FingerprintInfo],
        ['query'],
      ),
    'get_pool' : IDL.Func([IDL.Opt(IDL.Nat32)], [PoolInfo], ['query']),
    'get_pool_address' : IDL.Func([IDL.Opt(IDL.Nat32)], [IDL.Text], ['query']),
    'get_processing' : IDL.Func([], [IDL.Vec(IDL.Text)], ['query']),
    'get_recent_unstake_records' : IDL.Func(
        [],
        [IDL.Vec(UnstakeRecord)],
        ['query'],
      ),
    'get_recorded' : IDL.Func([], [IDL.Vec(TxRecord)], ['query']),
    'get_tx' : IDL.Func([IDL.Text], [Result_2], []),
    'get_user_latest_unstake_record' : IDL.Func(
        [IDL.Text],
        [IDL.Opt(UnstakeRecord)],
        ['query'],
      ),
    'get_xpub' : IDL.Func([IDL.Opt(IDL.Nat32)], [IDL.Text], ['query']),
    'initialize_pool_addresses_range' : IDL.Func(
        [IDL.Nat32, IDL.Nat32],
        [Result_2],
        [],
      ),
    'insert_record' : IDL.Func([TxRecord], [], []),
    'parse_psbt_runes' : IDL.Func([IDL.Text], [IDL.Text], []),
    'remove_processing' : IDL.Func([IDL.Text], [], []),
    'reset_exchange_rate' : IDL.Func([], [], []),
    'stake' : IDL.Func([IDL.Text], [IDL.Text], []),
    'start_cron' : IDL.Func([], [], []),
    'unstake' : IDL.Func([IDL.Text], [IDL.Text], []),
    'withdraw' : IDL.Func([IDL.Text], [IDL.Text], []),
  });
};
export const init = ({ IDL }) => { return []; };
