use crate::{
    commands::{CoinStatus, LabelItem},
    jsonrpc::{Error, Params, Request, Response},
    DaemonControl,
};

use std::{
    collections::{HashMap, HashSet},
    convert::TryInto,
    str::FromStr,
};

use miniscript::bitcoin::{self, psbt::PartiallySignedTransaction as Psbt};

fn create_spend(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let destinations = params
        .get(0, "destinations")
        .ok_or_else(|| Error::invalid_params("Missing 'destinations' parameter."))?
        .as_object()
        .and_then(|obj| {
            obj.into_iter()
                .map(|(k, v)| {
                    let addr = bitcoin::Address::from_str(k).ok()?;
                    let amount: u64 = v.as_i64()?.try_into().ok()?;
                    Some((addr, amount))
                })
                .collect::<Option<HashMap<bitcoin::Address<bitcoin::address::NetworkUnchecked>, u64>>>()
        })
        .ok_or_else(|| Error::invalid_params("Invalid 'destinations' parameter."))?;
    let outpoints = params
        .get(1, "outpoints")
        .ok_or_else(|| Error::invalid_params("Missing 'outpoints' parameter."))?
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .map(|entry| {
                    entry
                        .as_str()
                        .and_then(|e| bitcoin::OutPoint::from_str(e).ok())
                })
                .collect::<Option<Vec<bitcoin::OutPoint>>>()
        })
        .ok_or_else(|| Error::invalid_params("Invalid 'outpoints' parameter."))?;
    let feerate: u64 = params
        .get(2, "feerate")
        .ok_or_else(|| Error::invalid_params("Missing 'feerate' parameter."))?
        .as_u64()
        .ok_or_else(|| Error::invalid_params("Invalid 'feerate' parameter."))?;

    let res = control.create_spend(&destinations, &outpoints, feerate)?;
    Ok(serde_json::json!(&res))
}

fn update_spend(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let psbt: Psbt = params
        .get(0, "psbt")
        .ok_or_else(|| Error::invalid_params("Missing 'psbt' parameter."))?
        .as_str()
        .and_then(|s| Psbt::from_str(s).ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'psbt' parameter."))?;
    control.update_spend(psbt)?;

    Ok(serde_json::json!({}))
}

fn delete_spend(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let txid = params
        .get(0, "txid")
        .ok_or_else(|| Error::invalid_params("Missing 'txid' parameter."))?
        .as_str()
        .and_then(|s| bitcoin::Txid::from_str(s).ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'txid' parameter."))?;
    control.delete_spend(&txid);

    Ok(serde_json::json!({}))
}

fn broadcast_spend(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let txid = params
        .get(0, "txid")
        .ok_or_else(|| Error::invalid_params("Missing 'txid' parameter."))?
        .as_str()
        .and_then(|s| bitcoin::Txid::from_str(s).ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'txid' parameter."))?;
    control.broadcast_spend(&txid)?;

    Ok(serde_json::json!({}))
}

fn list_coins(control: &DaemonControl, params: Option<Params>) -> Result<serde_json::Value, Error> {
    let statuses_arg = params
        .as_ref()
        .and_then(|p| p.get(0, "statuses"))
        .and_then(|statuses| statuses.as_array());
    let statuses: Vec<CoinStatus> = if let Some(statuses_arg) = statuses_arg {
        statuses_arg
            .iter()
            .map(|status_arg| {
                status_arg
                    .as_str()
                    .and_then(CoinStatus::from_arg)
                    .ok_or_else(|| {
                        Error::invalid_params(format!(
                            "Invalid value {} in 'statuses' parameter.",
                            status_arg
                        ))
                    })
            })
            .collect::<Result<Vec<CoinStatus>, Error>>()?
    } else {
        Vec::new()
    };
    let outpoints_arg = params
        .as_ref()
        .and_then(|p| p.get(1, "outpoints"))
        .and_then(|op| op.as_array());
    let outpoints: Vec<bitcoin::OutPoint> = if let Some(outpoints_arg) = outpoints_arg {
        outpoints_arg
            .iter()
            .map(|op_arg| {
                op_arg
                    .as_str()
                    .and_then(|op| bitcoin::OutPoint::from_str(op).map_or_else(|_| None, Some))
                    .ok_or_else(|| {
                        Error::invalid_params(format!(
                            "Invalid value {} in 'outpoints' parameter.",
                            op_arg
                        ))
                    })
            })
            .collect::<Result<Vec<bitcoin::OutPoint>, Error>>()?
    } else {
        Vec::new()
    };
    let res = control.list_coins(&statuses, &outpoints);
    Ok(serde_json::json!(&res))
}

fn list_confirmed(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let start: u32 = params
        .get(0, "start")
        .ok_or_else(|| Error::invalid_params("Missing 'start' parameter."))?
        .as_i64()
        .and_then(|i| i.try_into().ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'start' parameter."))?;

    let end: u32 = params
        .get(1, "end")
        .ok_or_else(|| Error::invalid_params("Missing 'end' parameter."))?
        .as_i64()
        .and_then(|i| i.try_into().ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'end' parameter."))?;

    let limit: u64 = params
        .get(2, "limit")
        .ok_or_else(|| Error::invalid_params("Missing 'limit' parameter."))?
        .as_i64()
        .and_then(|i| i.try_into().ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'limit' parameter."))?;

    Ok(serde_json::json!(
        &control.list_confirmed_transactions(start, end, limit)
    ))
}

fn list_transactions(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let txids: Vec<bitcoin::Txid> = params
        .get(0, "txids")
        .ok_or_else(|| Error::invalid_params("Missing 'txids' parameter."))?
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .map(|entry| entry.as_str().and_then(|e| bitcoin::Txid::from_str(e).ok()))
                .collect()
        })
        .ok_or_else(|| Error::invalid_params("Invalid 'txids' parameter."))?;
    Ok(serde_json::json!(&control.list_transactions(&txids)))
}

fn start_rescan(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let timestamp: u32 = params
        .get(0, "timestamp")
        .ok_or_else(|| Error::invalid_params("Missing 'timestamp' parameter."))?
        .as_u64()
        .and_then(|t| t.try_into().ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'timestamp' parameter."))?;
    control.start_rescan(timestamp)?;

    Ok(serde_json::json!({}))
}

fn create_recovery(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let address = params
        .get(0, "address")
        .ok_or_else(|| Error::invalid_params("Missing 'address' parameter."))?
        .as_str()
        .and_then(|s| bitcoin::Address::from_str(s).ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'address' parameter."))?;
    let feerate: u64 = params
        .get(1, "feerate")
        .ok_or_else(|| Error::invalid_params("Missing 'feerate' parameter."))?
        .as_u64()
        .ok_or_else(|| Error::invalid_params("Invalid 'feerate' parameter."))?;
    let timelock: Option<u16> = params
        .get(2, "timelock")
        .map(|tl| {
            tl.as_u64()
                .and_then(|tl| tl.try_into().ok())
                .ok_or_else(|| Error::invalid_params("Invalid 'timelock' parameter."))
        })
        .transpose()?;

    let res = control.create_recovery(address, feerate, timelock)?;
    Ok(serde_json::json!(&res))
}

fn update_labels(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let mut items = HashMap::new();
    for (item, value) in params
        .get(0, "labels")
        .ok_or_else(|| Error::invalid_params("Missing 'labels' parameter."))?
        .as_object()
        .ok_or_else(|| Error::invalid_params("Invalid 'labels' parameter."))?
        .iter()
    {
        let value = value
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::invalid_params(format!("Invalid 'labels.{}' value.", item)))?;
        if value.len() > 100 {
            return Err(Error::invalid_params(format!(
                "Invalid 'labels.{}' value length: must be less or equal than 100 characters",
                item
            )));
        }
        let item =
            LabelItem::from_str(item, control.config.bitcoin_config.network).ok_or_else(|| {
                Error::invalid_params(format!(
                    "Invalid 'labels.{}' parameter: must be an address, a txid or an outpoint",
                    item
                ))
            })?;
        items.insert(item, value);
    }

    control.update_labels(&items);
    Ok(serde_json::json!({}))
}

fn get_labels(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let mut items = HashSet::new();
    for item in params
        .get(0, "items")
        .ok_or_else(|| Error::invalid_params("Missing 'items' parameter."))?
        .as_array()
        .ok_or_else(|| Error::invalid_params("Invalid 'items' parameter."))?
        .iter()
    {
        let item = item.as_str().ok_or_else(|| {
            Error::invalid_params(format!(
                "Invalid item {} format: must be an address, a txid or an outpoint",
                item
            ))
        })?;

        let item =
            LabelItem::from_str(item, control.config.bitcoin_config.network).ok_or_else(|| {
                Error::invalid_params(format!(
                    "Invalid item {} format: must be an address, a txid or an outpoint",
                    item
                ))
            })?;
        items.insert(item);
    }

    Ok(serde_json::json!(control.get_labels(&items)))
}

/// Handle an incoming JSONRPC2 request.
pub fn handle_request(control: &DaemonControl, req: Request) -> Result<Response, Error> {
    let result = match req.method.as_str() {
        "broadcastspend" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'txid' parameter."))?;
            broadcast_spend(control, params)?
        }
        "createrecovery" => {
            let params = req.params.ok_or_else(|| {
                Error::invalid_params("Missing 'address' and 'feerate' parameters.")
            })?;
            create_recovery(control, params)?
        }
        "createspend" => {
            let params = req.params.ok_or_else(|| {
                Error::invalid_params(
                    "Missing 'outpoints', 'destinations' and 'feerate' parameters.",
                )
            })?;
            create_spend(control, params)?
        }
        "delspendtx" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'txid' parameter."))?;
            delete_spend(control, params)?
        }
        "getinfo" => serde_json::json!(&control.get_info()),
        "getnewaddress" => serde_json::json!(&control.get_new_address()),
        "listcoins" => {
            let params = req.params;
            list_coins(control, params)?
        }
        "listconfirmed" => {
            let params = req.params.ok_or_else(|| {
                Error::invalid_params(
                    "The 'listconfirmed' command requires 3 parameters: 'start', 'end' and 'limit'",
                )
            })?;
            list_confirmed(control, params)?
        }
        "listspendtxs" => serde_json::json!(&control.list_spend()),
        "listtransactions" => {
            let params = req.params.ok_or_else(|| {
                Error::invalid_params(
                    "The 'listtransactions' command requires 1 parameter: 'txids'",
                )
            })?;
            list_transactions(control, params)?
        }
        "startrescan" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'timestamp' parameter."))?;
            start_rescan(control, params)?
        }
        "stop" => serde_json::json!({}),
        "updatespend" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'psbt' parameter."))?;
            update_spend(control, params)?
        }
        "updatelabels" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'labels' parameter."))?;
            update_labels(control, params)?
        }
        "getlabels" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'items' parameter."))?;
            get_labels(control, params)?
        }
        _ => {
            return Err(Error::method_not_found());
        }
    };

    Ok(Response::success(req.id, result))
}
