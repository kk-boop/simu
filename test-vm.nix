let
  pkgs = import <nixpkgs> {};
  simu = pkgs.callPackage ./simu.nix {};
  noGraphics = {
    virtualisation.graphics = false;
  };
in pkgs.nixosTest({
  name = "simu";

  nodes = {
    server = { config, pkgs, ... }: {
      imports = [ noGraphics ];
      networking.firewall.allowedTCPPorts = [ 8080 ];
      users = {
        mutableUsers = false;
        users.testaccount = {
          initialPassword = "testpassword";
          isNormalUser = true;
        };
      };

      systemd.services.simu = {
        serviceConfig = {
          ExecStart = "${simu}/bin/simu";
          Type = "simple";
          WorkingDirectory = "/data";
          User = "root";
        };
      };
      environment.systemPackages = [ simu pkgs.pam ];
    };
    client = { ... }: {
      imports = [ noGraphics ];
      environment.systemPackages = [ pkgs.curl ];
    };
  };

  testScript = ''
    # start client in parallel
    start_all()

    # prepare data for test
    server.succeed("mkdir /data")
    server.succeed("echo test > /data/test")
    server.succeed("chown testaccount:root /data/test")
    server.succeed("chmod 600 /data/test")

    # start service and wait until it's available
    server.succeed("systemctl start simu")
    server.wait_for_unit("simu")

    # attempt requests from the client vm
    client.succeed('curl --fail -o - testaccount:testpassword@server:8080/test')
    client.fail('curl --fail -o - testaccount:testpassword@server:8080/nonexistant')
    client.fail('curl --fail -o - notanaccount:testpassword@server:8080/nonexistant')
  '';
})
