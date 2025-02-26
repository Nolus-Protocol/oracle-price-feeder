use std::{
    borrow::Borrow, collections::BTreeMap, fmt::Debug, future::Future,
    marker::PhantomData, sync::Arc,
};

use anyhow::Result;
use serde::Deserialize;

use chain_ops::node;

use crate::Currencies;

pub trait Dex: Send + Sync + Sized + 'static {
    type ProviderTypeDescriptor;

    type AssociatedPairData: for<'r> Deserialize<'r> + Send + Sync + 'static;

    type PriceQueryMessage: Send + 'static;

    const PROVIDER_TYPE: Self::ProviderTypeDescriptor;

    #[inline]
    fn price_query_messages<Pairs, Ticker>(
        &self,
        pairs: Pairs,
        currencies: &Currencies,
    ) -> Result<BTreeMap<CurrencyPair<Ticker>, Self::PriceQueryMessage>>
    where
        Self: Dex<AssociatedPairData = ()>,
        Pairs: IntoIterator<Item = CurrencyPair<Ticker>>,
        Ticker: Borrow<str> + Ord,
    {
        self.price_query_messages_with_associated_data(
            pairs.into_iter().map(
                #[inline]
                |pair| (pair, ()),
            ),
            currencies,
        )
    }

    fn price_query_messages_with_associated_data<
        Pairs,
        Ticker,
        AssociatedPairData,
    >(
        &self,
        pairs: Pairs,
        currencies: &Currencies,
    ) -> Result<BTreeMap<CurrencyPair<Ticker>, Self::PriceQueryMessage>>
    where
        Pairs: IntoIterator<Item = (CurrencyPair<Ticker>, AssociatedPairData)>,
        Ticker: Borrow<str> + Ord,
        AssociatedPairData: Borrow<Self::AssociatedPairData>;

    fn price_query(
        &self,
        dex_node_client: &node::Client,
        query_message: &Self::PriceQueryMessage,
    ) -> impl Future<Output = Result<(Amount<Base>, Amount<Quote>)>> + Send + 'static;
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decimal {
    amount: String,
    decimal_places: u8,
}

impl Decimal {
    #[inline]
    pub const fn new(amount: String, decimal_places: u8) -> Self {
        Self {
            amount,
            decimal_places,
        }
    }

    #[inline]
    #[must_use]
    pub fn amount(&self) -> &str {
        &self.amount
    }

    #[inline]
    #[must_use]
    pub fn into_amount(self) -> String {
        self.amount
    }

    #[inline]
    #[must_use]
    pub const fn decimal_places(&self) -> u8 {
        self.decimal_places
    }
}

pub trait Marker: Debug + Copy + Eq {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {}

impl Marker for Base {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quote {}

impl Marker for Quote {}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Amount<T>
where
    T: Marker,
{
    amount: Decimal,
    _marker: PhantomData<T>,
}

impl<T> Amount<T>
where
    T: Marker,
{
    #[inline]
    pub const fn new(amount: Decimal) -> Self {
        Self {
            amount,
            _marker: const { PhantomData },
        }
    }

    #[inline]
    pub const fn as_inner(&self) -> &Decimal {
        &self.amount
    }

    #[inline]
    pub fn into_inner(self) -> Decimal {
        self.amount
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CurrencyPair<T = Arc<str>>
where
    T: Borrow<str>,
{
    pub base: T,
    pub quote: T,
}
