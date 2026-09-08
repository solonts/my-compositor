{
  description = "Smithay Compositor Dev Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
	pkgs = import nixpkgs { inherit system; };

	smithayDeps = with pkgs; [
	  cargo
	  rustc
	  rust-analyzer
	  pkg-config
	  wayland-utils
	  weston

	  # Wayland & Input Libs
	  wayland
	  wayland-protocols
	  libxkbcommon
	  pixman
	  udev
	  dbus

	  #Graphic Libs
	  libdrm
	  mesa
	  vulkan-loader
	  libGL

	  #Seat & Session management
	  systemd # for libsystemd / logind integration
	  seatd
	];

	ldLibraryPath = pkgs.lib.makeLibraryPath smithayDeps;

      in {
	# Test Level 1 Standard dev shell for Winit / Host development Usage: `nix develop`
	devShells.default = pkgs.mkShell {
	  nativeBuildInputs = smithayDeps;

	  LD_LIBRARY_PATH = ldLibraryPath;
	  RUST_BACKTRACE = 1;

	  shellHook = ''
	    echo "--- Smithay Level 1 Dev Environment Loaded ---"
	    echo "Run 'cargo run' to test you compositor with the Winit backend."
	  '';
	};

});
/*
	packages.default = pkgs.rustPlatform.buildRustPackage {
	  pname = "my-compositor";
	  version = "0.1.0";
	  src = ./.;

	  cargoLock.lockFile = ./Cargo.lock;

	  nativeBuildInputs = [ pkgs.pkg-config ];
	  buildInputs = smithayDeps;

	  postInstall = ''
	    # Ensure the binary can locate shared libs at runtime
	    patchelf --set-rpath "${ldLibraryPath}" $out/bin/*
	  '';
	};

      }) // {

	# Test Level 2 QEMU VM Setup (NixOS Config) Usage: `nix run .#vm`
	nixosConfigurations.vm = nixpkgs.lib.nixosSystem {
	  system = "x86_64-linux";
	  modules = [
	    ({ pkgs, config, ... }: {
	      virtualisation = {
		memorySize = 2048; #MB
		cores = 2;
		qemu.options = [
		  "-enable-kvm"
		  "-vga virtio"
		  "-display gkt,gl=on" # Enable OpenGL acceleration
		];
	      };

	      services.getty.autologinUser = "root";

	      environment.systemPackages = [
		self.packages.x86_64-linux.default
		pkgs.foot
		pkgs.mesa-demos
	      ];

	      hardware.graphics.enable = true;
	      services.udev.enable = true;

	      environment.loginShellInit = ''
		if [ "$(tty)" = "/dev/tty1" ]; then
		  echo "Starting Smithay Compositor (udev backend)..."
		  # Replace with binary name
		  exec my-compositor --backend udev
		fi
	      '';
	    })
	  ];
	};

	apps.x86_64-linux.vm = {
	  type = "app";
	  program = "${self.nixosConfigurations.vm.config.system.build.vm}/bin/run-nixos-vm";
	};
      };
      */
}



