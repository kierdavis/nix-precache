FROM nixos/nix:2.3

# Set up nix
RUN nix-channel --add https://nixos.org/channels/nixpkgs-unstable nixpkgs \
 && nix-channel --update

# Install standard packages
RUN nix-env -iA nixpkgs.python2Packages.supervisor nixpkgs.nix-serve nixpkgs.curl

# Install channel-proxy
ADD channel-proxy /tmp/channel-proxy-build
# RUN nix-env -i $(nix-instantiate -E 'with import <nixpkgs> {}; callPackage /tmp/channel-proxy-build {}') \
#  && rm -rf /tmp/channel-proxy-build
RUN nix-shell -p cargo --run 'cd /tmp/channel-proxy-build && cargo build --release' \
 && cp /tmp/channel-proxy-build/target/release/channel-proxy /bin/channel-proxy \
 && rm -rf /tmp/channel-proxy-build

# Add config files
ADD supervisord.conf /etc/supervisord.conf

# nix-serve
EXPOSE 5000
# channel-proxy
EXPOSE 8000

ENTRYPOINT ["supervisord", "-c", "/etc/supervisord.conf"]
