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

//! # NFT Lending Pallet

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

mod types;

#[cfg(test)]
pub mod mock;
#[cfg(test)]
mod tests;

use frame_system::Config as SystemConfig;

pub use pallet::*;
pub use scale_info::Type;
pub use types::*;

/// The log target of this pallet.
pub const LOG_TARGET: &'static str = "runtime::nfts-lending";

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use crate::types::{DepositOf, Details};

	use frame_support::{
		dispatch::DispatchResult,
		ensure,
		pallet_prelude::*,
		sp_runtime::traits::AccountIdConversion,
		traits::{
			fungible::{
				hold::Mutate as HoldMutateFungible, Inspect as InspectFungible,
				Mutate as MutateFungible,
			},
			tokens::{
				nonfungibles_v2::{Inspect as NonFungiblesInspect, Transfer},
				Precision::BestEffort,
			},
			VestingSchedule,
		},
		PalletId,
	};
	use frame_system::pallet_prelude::*;
	use sp_std::{fmt::Display, prelude::*};

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency mechanism, used for paying for deposits.
		type Currency: InspectFungible<Self::AccountId>
			+ MutateFungible<Self::AccountId>
			+ HoldMutateFungible<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// Overarching hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// The deposit paid by the user locking an NFT. The deposit is returned to the original NFT
		/// owner when the NFT is removed from lending.
		#[pallet::constant]
		type Deposit: Get<DepositOf<Self>>;

		/// Identifier for the collection of NFT.
		type NftCollectionId: Member + Parameter + MaxEncodedLen + Copy + Display;

		/// The type used to identify an NFT within a collection.
		type NftId: Member + Parameter + MaxEncodedLen + Copy + Display;

		/// Registry for minted NFTs.
		type Nfts: NonFungiblesInspect<
				Self::AccountId,
				ItemId = Self::NftId,
				CollectionId = Self::NftCollectionId,
			> + Transfer<Self::AccountId>;

		/// The pallet's id, used for deriving its sovereign account ID.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		// The vesting schedule for borrowed NFTs.
		// type VestingSchedule: VestingSchedule<Self::AccountId, Moment = BlockNumberFor<Self>>;
	}

	/// Storage for Nfts that are lendable
	#[pallet::storage]
	#[pallet::getter(fn get_lendable_nfts)]
	pub type LendableNfts<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		(T::NftCollectionId, T::NftId),
		Details<BalanceOf<T>, DepositOf<T>, T::AccountId>,
		OptionQuery,
	>;

	// Storage for lendable Nfts that are currently lent
	#[pallet::storage]
	#[pallet::getter(fn get_lent_nfts)]
	pub type LentNfts<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		(T::NftCollectionId, T::NftId),
		BorrowingDetails<T::AccountId>,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An NFT has been made lendable.
		Lendable {
			nft_collection: T::NftCollectionId,
			nft_id: T::NftId,
			min_period: u64,
			max_period: u64,
			price_per_block: BalanceOf<T>,
		},
		/// An NFT has been lent.
		Lent {
			nft_collection: T::NftCollectionId,
			nft_id: T::NftId,
			borrowing_period: u64,
			borrower: T::AccountId,
		},
		// An NFT has been removed from being lendable.
		NotLendable {
			nft_collection: T::NftCollectionId,
			nft_id: T::NftId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The NFT is not lendable.
		LendableNftNotFound,
		/// The NFT does not exist.
		NftNotFound,
		/// The signing account has no permission to do the operation.
		NoPermission,
		/// The NFT is already lent.
		NftAlreadyLent,
		/// The borrowing period is less than the minimum period.
		BorrowingPeriodLessThanMinPeriod,
		/// The borrowing period is greater than the maximum period.
		BorrowingPeriodGreaterThanMaxPeriod,
		/// The min_period a NFT can be lent is greater than max_period.
		MinPeriodGreaterThanMaxPeriod,
	}

	/// A reason for the pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Reserved for making an NFT lendable.
		#[codec(index = 0)]
		LendableNft,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Lock the NFT and make it lendable.
		///
		/// The dispatch origin for this call must be Signed.
		/// The origin must be the owner of the NFT.
		///
		/// `Deposit` funds of sender are reserved.
		///
		/// - `nft_collection_id`: The ID used to identify the collection of the NFT.
		/// Is used within the context of `pallet_nfts`.
		/// - `nft_id`: The ID used to identify the NFT within the given collection.
		///
		/// - `min_period`: The minimum period (in number of blocks) for which the NFT can be lent.
		/// - `max_period`: The maximum period (in number of blocks) for which the NFT can be lent.
		/// - `price_per_block`: The price per block for lending the NFT.
		///
		/// Emits `Lendable` event when successful.
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn list_nft(
			origin: OriginFor<T>,
			nft_collection_id: T::NftCollectionId,
			nft_id: T::NftId,
			min_period: u64,
			max_period: u64,
			price_per_block: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let nft_owner =
				T::Nfts::owner(&nft_collection_id, &nft_id).ok_or(Error::<T>::NftNotFound)?;
			ensure!(nft_owner == who, Error::<T>::NoPermission);

			ensure!(min_period < max_period, Error::<T>::MinPeriodGreaterThanMaxPeriod);

			let deposit = T::Deposit::get();
			T::Currency::hold(&HoldReason::LendableNft.into(), &nft_owner, deposit)?;

			// Alternative Strategy
			// Call NFTs pallet `approve_transfer` to allow the pallet account to transfer the NFT
			let _ = T::Nfts::transfer(&nft_collection_id, &nft_id, &Self::get_pallet_account());

			Self::do_lock_nft(nft_collection_id, nft_id)?;

			LendableNfts::<T>::insert(
				(nft_collection_id, nft_id),
				Details {
					min_period,
					max_period,
					price_per_block,
					deposit,
					deposit_owner: nft_owner.clone(),
					nft_owner: nft_owner.clone(),
				},
			);

			Self::deposit_event(Event::Lendable {
				nft_collection: nft_collection_id,
				nft_id,
				min_period,
				max_period,
				price_per_block,
			});

			Ok(())
		}

		/// Borrow an NFT that is the `LendableNfts` storage for the `borrowing_period` number of
		/// blocks.
		///
		/// The dispatch origin for this call must be Signed.
		///
		/// `Deposit` funds will be returned to `asset_creator`.
		///
		/// - `nft_collection_id`: The ID used to identify the collection of the NFT.
		/// Is used within the context of `pallet_nfts`.
		/// - `nft_id`: The ID used to identify the NFT within the given collection.
		/// - `borrowing_period`: The period (in number of blocks) for which the NFT is borrowed.
		///
		/// Emits `NftLent` event when successful.
		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn borrow_nft(
			origin: OriginFor<T>,
			nft_collection_id: T::NftCollectionId,
			nft_id: T::NftId,
			borrowing_period: u64,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Ensure that the NFT exists in the LendableNfts storage
			ensure!(
				LendableNfts::<T>::contains_key((nft_collection_id, nft_id)),
				Error::<T>::LendableNftNotFound
			);

			// Ensure that the NFT is not already lent
			ensure!(
				!LentNfts::<T>::contains_key((nft_collection_id, nft_id)),
				Error::<T>::NftAlreadyLent
			);

			// Get the min_period and max_period from the `LendableNfts` storage
			let Details { min_period, max_period, price_per_block, .. } =
				LendableNfts::<T>::get((nft_collection_id, nft_id))
					.ok_or(Error::<T>::LendableNftNotFound)?;

			ensure!(borrowing_period >= min_period, Error::<T>::BorrowingPeriodLessThanMinPeriod);
			ensure!(
				borrowing_period <= max_period,
				Error::<T>::BorrowingPeriodGreaterThanMaxPeriod
			);

			// Add Lendable NFT to `LentNfts` storage
			LentNfts::<T>::insert(
				(nft_collection_id, nft_id),
				BorrowingDetails { borrowing_period, borrower: who.clone() },
			);

			// Strategy #1
			// Unlock the NFT
			// Call the NFTs pallet `transfer` to transfer the NFT to the borrower
			// And set this pallet as an approved account to transfer the NFT
			// Problem: The borrower could remove the pallet as the approval delegate and steal the
			// NFT

			// Strategy #2
			// Create a `force_transfer` function in the NFTs pallet with this pallet as the origin

			// Strategy #3
			// Transfer the ownership of the NFT to this pallet account
			// Set the borrower of the NFT in the LentNfts storage
			// Return ownership of the NFT to the lender when the borrowing period ends
			// Perhaps we can piggy-back off the vesting logic to know when the borrowing period
			// ends This is permissionless and the borrower cannot steal the NFT

			// TODO: Vesting logic to be added here
			// Vest for the borrowing period with the percentage set to the details.price_per_block
			// for the lendable NFT https://paritytech.github.io/polkadot-sdk/master/frame_support/traits/tokens/currency/trait.VestingSchedule.html
			// Call T::VestingSchedule::add_vesting_schedule with the borrower as the account where
			// locked is equal to the price_per_block and per_block is equal to the price_per_block
			// is equal to the price_per_block and the starting block is equal to the current block
			// number +1 T::VestingSchedule::add_vesting_schedule(
			// 	&who,
			// 	price_per_block,
			// 	price_per_block,
			// 	frame_system::Pallet::<T>::block_number(),
			// );

			Self::deposit_event(Event::Lent {
				nft_collection: nft_collection_id,
				nft_id,
				borrowing_period,
				borrower: who.clone(),
			});

			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn remove_from_lending(
			origin: OriginFor<T>,
			nft_collection_id: T::NftCollectionId,
			nft_id: T::NftId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let Details { deposit, deposit_owner, nft_owner, .. } =
				LendableNfts::<T>::get((nft_collection_id, nft_id))
					.ok_or(Error::<T>::LendableNftNotFound)?;

			ensure!(nft_owner == who.clone(), Error::<T>::NoPermission);

			Self::do_unlock_nft(nft_collection_id, nft_id)?;
			let _ = T::Nfts::transfer(&nft_collection_id, &nft_id, &who);

			LendableNfts::<T>::remove((nft_collection_id, nft_id));

			T::Currency::release(
				&HoldReason::LendableNft.into(),
				&deposit_owner,
				deposit,
				BestEffort,
			)?;

			Self::deposit_event(Event::NotLendable { nft_collection: nft_collection_id, nft_id });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The account ID of the pallet.
		///
		/// This actually does computation. If you need to keep using it, then make sure you cache
		/// the value and only call this once.
		pub fn get_pallet_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Lock the NFT preventing transfers
		fn do_lock_nft(nft_collection_id: T::NftCollectionId, nft_id: T::NftId) -> DispatchResult {
			T::Nfts::disable_transfer(&nft_collection_id, &nft_id)
		}

		/// Unlock the NFT enabling transfers
		fn do_unlock_nft(
			nft_collection_id: T::NftCollectionId,
			nft_id: T::NftId,
		) -> DispatchResult {
			T::Nfts::enable_transfer(&nft_collection_id, &nft_id)
		}
	}
}
