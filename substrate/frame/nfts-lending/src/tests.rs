// This file is part of Substrate.

// Copyright (C) 2019-2022 Parity Technologies (UK) Ltd.
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

//! Tests for the NFTs Lending pallet.

use crate::{mock::*, Error, Event as NftsLendingEvent, *};
use frame_support::{assert_noop, assert_ok, traits::fungible::Mutate as MutateFungible};

use pallet_nfts::{
	Account, CollectionAccount, CollectionConfig, CollectionSetting, CollectionSettings,
	MintSettings,
};
pub use sp_runtime::{DispatchError, ModuleError, Permill};

type AccountIdOf<Test> = <Test as frame_system::Config>::AccountId;
fn account(id: u8) -> AccountIdOf<Test> {
	[id; 32].into()
}

fn last_event() -> NftsLendingEvent<Test> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::NftsLending(inner) = e { Some(inner) } else { None })
		.last()
		.unwrap()
}

// Create a collection calling directly the NFTs pallet
fn create_collection() {
	assert_ok!(Nfts::force_create(
		RuntimeOrigin::root(),
		account(1),
		CollectionConfig {
			settings: CollectionSettings::from_disabled(CollectionSetting::DepositRequired.into()),
			max_supply: None,
			mint_settings: MintSettings::default(),
		}
	));
	let mut collections: Vec<_> = CollectionAccount::<Test>::iter().map(|x| (x.0, x.1)).collect();
	collections.sort();
	assert_eq!(collections, vec![(account(1), 0)]);
}

fn mint_item() -> u32 {
	let item_id = 42;
	assert_ok!(Nfts::mint(RuntimeOrigin::signed(account(1)), 0, item_id, account(1), None,));
	item_id
}

// Set up balances for testing
fn set_up_balances(initial_balance: u64) {
	Balances::set_balance(&account(1), initial_balance);
	Balances::set_balance(&account(2), initial_balance);
	Balances::set_balance(&account(3), initial_balance);
	Balances::set_balance(&account(4), initial_balance);
}

#[test]
fn list_nft_should_work() {
	new_test_ext().execute_with(|| {
		let initial_balance = 100;
		set_up_balances(initial_balance);
		create_collection();
		let mint_id = mint_item();
		assert_ok!(NftsLending::list_nft(
			RuntimeOrigin::signed(account(1)),
			0,
			mint_id,
			2,
			10,
			100,
		));
		let pallet_account = NftsLending::get_pallet_account();
		// Check that the pallet is now the owner of the NFT again
		assert_eq!(Nfts::owner(0, mint_id), Some(pallet_account.clone()));

		// Get the items directly from the NFTs pallet, to see if has been created there
		let mut items: Vec<_> = Account::<Test>::iter().map(|x| x.0).collect();
		items.sort();
		assert_eq!(items, vec![(pallet_account, 0, mint_id)]);

		// Read royalty pallet's storage.
		let lendable_nft = LendableNfts::<Test>::get((0, mint_id)).unwrap();
		assert_eq!(lendable_nft.min_period, 2);
		assert_eq!(lendable_nft.max_period, 10);
		assert_eq!(lendable_nft.price_per_block, 100);
		assert_eq!(lendable_nft.deposit_owner, account(1));
		assert_eq!(lendable_nft.deposit, 1);

		// Check that the deposit was taken
		assert_eq!(Balances::free_balance(&account(1)), 99);

		// Check the event was emitted
		assert_eq!(
			last_event(),
			NftsLendingEvent::Lendable {
				nft_collection: 0,
				nft_id: mint_id,
				min_period: 2,
				max_period: 10,
				price_per_block: 100,
			}
		);
	});
}

#[test]
fn list_nft_should_fail_nft_no_exist() {
	new_test_ext().execute_with(|| {
		let initial_balance = 100;
		set_up_balances(initial_balance);
		create_collection();
		let _mint_id = mint_item();
		assert_noop!(
			NftsLending::list_nft(RuntimeOrigin::signed(account(1)), 0, 555, 2, 10, 100,),
			Error::<Test>::NftNotFound
		);
	});
}

#[test]
fn list_nft_should_fail_no_owner_nft() {
	new_test_ext().execute_with(|| {
		let initial_balance = 100;
		set_up_balances(initial_balance);
		create_collection();
		let mint_id = mint_item();
		assert_noop!(
			NftsLending::list_nft(RuntimeOrigin::signed(account(2)), 0, mint_id, 2, 10, 100,),
			Error::<Test>::NoPermission
		);
	});
}

#[test]
fn list_nft_should_fail_wrong_periods() {
	new_test_ext().execute_with(|| {
		let initial_balance = 100;
		set_up_balances(initial_balance);
		create_collection();
		let mint_id = mint_item();
		assert_noop!(
			NftsLending::list_nft(RuntimeOrigin::signed(account(1)), 0, mint_id, 20, 10, 100,),
			Error::<Test>::MinPeriodGreaterThanMaxPeriod
		);
	});
}
#[test]
fn remove_from_lending_should_work() {
	new_test_ext().execute_with(|| {
		let initial_balance = 100;
		set_up_balances(initial_balance);
		create_collection();
		let mint_id = mint_item();
		assert_ok!(NftsLending::list_nft(
			RuntimeOrigin::signed(account(1)),
			0,
			mint_id,
			2,
			10,
			100,
		));
		let pallet_account = NftsLending::get_pallet_account();
		// Get the items directly from the NFTs pallet, to see if has been created there
		let mut items: Vec<_> = Account::<Test>::iter().map(|x| x.0).collect();
		items.sort();
		assert_eq!(items, vec![(pallet_account, 0, mint_id)]);

		// Check that the deposit was taken
		assert_eq!(Balances::free_balance(&account(1)), 99);

		assert_ok!(
			NftsLending::remove_from_lending(RuntimeOrigin::signed(account(1)), 0, mint_id,)
		);

		// Check that the deposit is back
		assert_eq!(Balances::free_balance(&account(1)), 100);

		// Check that I am the owner of the NFT again
		assert_eq!(Nfts::owner(0, mint_id), Some(account(1)));

		// Check the event was emitted
		assert_eq!(
			last_event(),
			NftsLendingEvent::NotLendable { nft_collection: 0, nft_id: mint_id }
		);
	});
}
#[test]
fn remove_from_lending_should_fail_nft_not_listed() {
	new_test_ext().execute_with(|| {
		let initial_balance = 100;
		set_up_balances(initial_balance);
		create_collection();
		let mint_id = mint_item();
		assert_noop!(
			NftsLending::remove_from_lending(RuntimeOrigin::signed(account(1)), 0, mint_id),
			Error::<Test>::LendableNftNotFound
		);
	});
}

#[test]
fn remove_from_lending_should_fail_no_owner() {
	new_test_ext().execute_with(|| {
		let initial_balance = 100;
		set_up_balances(initial_balance);
		create_collection();
		let mint_id = mint_item();
		assert_ok!(NftsLending::list_nft(
			RuntimeOrigin::signed(account(1)),
			0,
			mint_id,
			2,
			10,
			100,
		));
		assert_noop!(
			NftsLending::remove_from_lending(RuntimeOrigin::signed(account(2)), 0, mint_id),
			Error::<Test>::NoPermission
		);
	});
}
