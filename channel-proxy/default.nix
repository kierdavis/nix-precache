{ buildPlatform, buildRustCrate, fetchgit, lib }:
(import ./Cargo.nix { inherit buildPlatform buildRustCrate fetchgit lib; }).channel_proxy {}
