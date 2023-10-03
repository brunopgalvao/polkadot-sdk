// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Various basic types for use in the NFT Lending Pallet.

use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::fungible::Inspect as InspectFungible;
use scale_info::TypeInfo;

pub type DepositOf<T> =
	<<T as Config>::Currency as InspectFungible<<T as SystemConfig>::AccountId>>::Balance;

pub type BalanceOf<T> =
	<<T as Config>::Currency as InspectFungible<<T as SystemConfig>::AccountId>>::Balance;

/// Stores the details of a lendable NFT.
#[derive(Decode, Encode, Default, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
pub struct Details<Balance, Deposit, AccountId> {
	/// The minimum number of blocks the NFT can be lent.
	pub min_period: u64,

	/// The maximum number of blocks the NFT can be lent.
	pub max_period: u64,

	/// The lending price per block.
	pub price_per_block: Balance,

	/// Reserved deposit for creating a new lendable NFT.
	pub deposit: Deposit,

	/// Account that created the lendable NFT.
	pub deposit_owner: AccountId,

	/// Account that owned the NFT before it was made lendable.
	pub nft_owner: AccountId,
}

/// Stores the details of a lendable NFT that is being borrowed.
#[derive(Decode, Encode, Default, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
pub struct BorrowingDetails<AccountId> {
	/// The number of blocks the NFT is being borrowed for.
	pub borrowing_period: u64,

	/// Account that borrowed the lendable NFT.
	pub borrower: AccountId,
}
