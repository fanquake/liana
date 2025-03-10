use chrono::NaiveDateTime;
use std::collections::HashMap;

use iced::{alignment, widget::Space, Alignment, Length};

use liana::miniscript::bitcoin;
use liana_ui::{
    color,
    component::{amount::*, button, card, event, form, text::*},
    icon, theme,
    util::Collection,
    widget::*,
};

use crate::{
    app::{
        cache::Cache,
        error::Error,
        menu::Menu,
        view::{coins, dashboard, label, message::Message},
    },
    daemon::model::HistoryTransaction,
};

pub const HISTORY_EVENT_PAGE_SIZE: u64 = 20;

pub fn home_view<'a>(
    balance: &'a bitcoin::Amount,
    unconfirmed_balance: &'a bitcoin::Amount,
    remaining_sequence: &Option<u32>,
    expiring_coins: &Vec<bitcoin::OutPoint>,
    pending_events: &'a [HistoryTransaction],
    events: &'a Vec<HistoryTransaction>,
) -> Element<'a, Message> {
    Column::new()
        .push(h3("Balance"))
        .push(
            Column::new()
                .push(amount_with_size(balance, H1_SIZE))
                .push_maybe(if unconfirmed_balance.to_sat() != 0 {
                    Some(
                        Row::new()
                            .spacing(10)
                            .push(text("+").size(H3_SIZE).style(color::GREY_3))
                            .push(unconfirmed_amount_with_size(unconfirmed_balance, H3_SIZE))
                            .push(text("unconfirmed").size(H3_SIZE).style(color::GREY_3)),
                    )
                } else {
                    None
                }),
        )
        .push_maybe(if expiring_coins.is_empty() {
            remaining_sequence.map(|sequence| {
                Container::new(
                    Row::new()
                        .spacing(15)
                        .align_items(Alignment::Center)
                        .push(
                            h4_regular(format!(
                                "≈ {} left before first recovery path becomes available.",
                                coins::expire_message_units(sequence).join(", ")
                            ))
                            .width(Length::Fill),
                        )
                        .push(
                            icon::tooltip_icon()
                                .size(20)
                                .style(color::GREY_3)
                                .width(Length::Fixed(20.0)),
                        )
                        .width(Length::Fill),
                )
                .padding(25)
                .style(theme::Card::Border)
            })
        } else {
            Some(
                Container::new(
                    Row::new()
                        .spacing(15)
                        .align_items(Alignment::Center)
                        .push(
                            h4_regular(format!(
                                "Recovery path is or will soon be available for {} coin(s).",
                                expiring_coins.len(),
                            ))
                            .width(Length::Fill),
                        )
                        .push(
                            button::primary(Some(icon::arrow_repeat()), "Refresh coins").on_press(
                                Message::Menu(Menu::RefreshCoins(expiring_coins.clone())),
                            ),
                        ),
                )
                .padding(25)
                .style(theme::Card::Invalid),
            )
        })
        .push(
            Column::new()
                .spacing(10)
                .push(h4_bold("Last payments"))
                .push(pending_events.iter().enumerate().fold(
                    Column::new().spacing(10),
                    |col, (i, event)| {
                        if !event.is_self_send() {
                            col.push(event_list_view(i, event))
                        } else {
                            col
                        }
                    },
                ))
                .push(events.iter().enumerate().fold(
                    Column::new().spacing(10),
                    |col, (i, event)| {
                        if !event.is_self_send() {
                            col.push(event_list_view(i + pending_events.len(), event))
                        } else {
                            col
                        }
                    },
                ))
                .push_maybe(
                    if events.len() % HISTORY_EVENT_PAGE_SIZE as usize == 0 && !events.is_empty() {
                        Some(
                            Container::new(
                                Button::new(
                                    text("See more")
                                        .width(Length::Fill)
                                        .horizontal_alignment(alignment::Horizontal::Center),
                                )
                                .width(Length::Fill)
                                .padding(15)
                                .style(theme::Button::TransparentBorder)
                                .on_press(Message::Next),
                            )
                            .width(Length::Fill)
                            .style(theme::Container::Card(theme::Card::Simple)),
                        )
                    } else {
                        None
                    },
                ),
        )
        .spacing(20)
        .into()
}

fn event_list_view(i: usize, event: &HistoryTransaction) -> Column<'_, Message> {
    event.tx.output.iter().enumerate().fold(
        Column::new().spacing(10),
        |col, (output_index, output)| {
            let label = if let Some(label) = event.labels.get(
                &bitcoin::OutPoint {
                    txid: event.tx.txid(),
                    vout: output_index as u32,
                }
                .to_string(),
            ) {
                Some(p1_bold(label))
            } else {
                event
                    .labels
                    .get(
                        &bitcoin::Address::from_script(&output.script_pubkey, event.network)
                            .unwrap()
                            .to_string(),
                    )
                    .map(|label| p1_bold(format!("address label: {}", label)).style(color::GREY_3))
            };
            if event.is_external() {
                if !event.change_indexes.contains(&output_index) {
                    col
                } else if let Some(t) = event.time {
                    col.push(event::confirmed_incoming_event(
                        label,
                        NaiveDateTime::from_timestamp_opt(t as i64, 0).unwrap(),
                        &Amount::from_sat(output.value),
                        Message::SelectSub(i, output_index),
                    ))
                } else {
                    col.push(event::unconfirmed_incoming_event(
                        label,
                        &Amount::from_sat(output.value),
                        Message::SelectSub(i, output_index),
                    ))
                }
            } else if event.change_indexes.contains(&output_index) {
                col
            } else if let Some(t) = event.time {
                col.push(event::confirmed_outgoing_event(
                    label,
                    NaiveDateTime::from_timestamp_opt(t as i64, 0).unwrap(),
                    &Amount::from_sat(output.value),
                    Message::SelectSub(i, output_index),
                ))
            } else {
                col.push(event::unconfirmed_outgoing_event(
                    label,
                    &Amount::from_sat(output.value),
                    Message::SelectSub(i, output_index),
                ))
            }
        },
    )
}

pub fn payment_view<'a>(
    cache: &'a Cache,
    tx: &'a HistoryTransaction,
    output_index: usize,
    labels_editing: &'a HashMap<String, form::Value<String>>,
    warning: Option<&'a Error>,
) -> Element<'a, Message> {
    let txid = tx.tx.txid().to_string();
    let outpoint = bitcoin::OutPoint {
        txid: tx.tx.txid(),
        vout: output_index as u32,
    }
    .to_string();
    dashboard(
        &Menu::Home,
        cache,
        warning,
        Column::new()
            .push(if tx.is_self_send() {
                Container::new(h3("Payment")).width(Length::Fill)
            } else if tx.is_external() {
                Container::new(h3("Incoming payment")).width(Length::Fill)
            } else {
                Container::new(h3("Outgoing payment")).width(Length::Fill)
            })
            .push(if let Some(label) = labels_editing.get(&outpoint) {
                label::label_editing(outpoint.clone(), label, H3_SIZE)
            } else {
                label::label_editable(outpoint.clone(), tx.labels.get(&outpoint), H1_SIZE)
            })
            .push(Container::new(amount_with_size(
                &Amount::from_sat(tx.tx.output[output_index].value),
                H1_SIZE,
            )))
            .push(Space::with_height(H3_SIZE))
            .push(Container::new(h3("Transaction")).width(Length::Fill))
            .push(if let Some(label) = labels_editing.get(&txid) {
                label::label_editing(txid.clone(), label, H3_SIZE)
            } else {
                label::label_editable(txid.clone(), tx.labels.get(&txid), H3_SIZE)
            })
            .push_maybe(tx.fee_amount.map(|fee_amount| {
                Row::new()
                    .align_items(Alignment::Center)
                    .push(h3("Miner fee: ").style(color::GREY_3))
                    .push(amount_with_size(&fee_amount, H3_SIZE))
                    .push(text(" ").size(H3_SIZE))
                    .push(
                        text(format!(
                            "({} sats/vbyte)",
                            fee_amount.to_sat() / tx.tx.vsize() as u64
                        ))
                        .size(H4_SIZE)
                        .style(color::GREY_3),
                    )
            }))
            .push(card::simple(
                Column::new()
                    .push_maybe(tx.time.map(|t| {
                        let date = NaiveDateTime::from_timestamp_opt(t as i64, 0)
                            .unwrap()
                            .format("%b. %d, %Y - %T");
                        Row::new()
                            .width(Length::Fill)
                            .push(Container::new(text("Date:").bold()).width(Length::Fill))
                            .push(Container::new(text(format!("{}", date))).width(Length::Shrink))
                    }))
                    .push(
                        Row::new()
                            .width(Length::Fill)
                            .align_items(Alignment::Center)
                            .push(Container::new(text("Txid:").bold()).width(Length::Fill))
                            .push(
                                Row::new()
                                    .align_items(Alignment::Center)
                                    .push(Container::new(text(format!("{}", tx.tx.txid())).small()))
                                    .push(
                                        Button::new(icon::clipboard_icon())
                                            .on_press(Message::Clipboard(tx.tx.txid().to_string()))
                                            .style(theme::Button::TransparentBorder),
                                    )
                                    .width(Length::Shrink),
                            ),
                    )
                    .spacing(5),
            ))
            .push(super::psbt::inputs_and_outputs_view(
                &tx.coins,
                &tx.tx,
                cache.network,
                if tx.is_external() {
                    None
                } else {
                    Some(tx.change_indexes.clone())
                },
                &tx.labels,
                labels_editing,
            ))
            .spacing(20),
    )
}
