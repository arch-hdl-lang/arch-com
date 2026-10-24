use std::fs;
use std::process::Command;

#[test]
fn chained_bus_aliases_preserve_and_override_params() {
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let fixture = std::env::temp_dir().join("arch_bug_hunt_r2_b04.arch");
    let output = fixture.with_extension("sv");
    fs::write(
        &fixture,
        r#"
bus B
  param W: const = 32;
  param D: const = 4;
  data: in UInt<W>;
  tag: in UInt<D>;
end bus B
module M
  type Narrow = B<W=8,D=2>;
  type Chain = Narrow;
  type ChainOverride = Chain<W=16>;
  port i: initiator Chain;
  port t: target Chain;
  port p: initiator Chain<D=3>;
  port q: initiator ChainOverride;
  wire w: Chain;
  port oi: out UInt<8>;
  comb
    oi = i.data;
    t.data = 0;
    t.tag = 0;
  end comb
end module M
"#,
    )
    .unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_arch"))
        .current_dir(root)
        .args([
            "build",
            fixture.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let sv = fs::read_to_string(&output).unwrap();
    assert!(sv.contains("input logic [7:0] i_data"));
    assert!(sv.contains("input logic [1:0] i_tag"));
    assert!(sv.contains("output logic [7:0] t_data"));
    assert!(sv.contains("output logic [1:0] t_tag"));
    assert!(sv.contains("input logic [7:0] p_data"));
    assert!(sv.contains("input logic [2:0] p_tag"));
    assert!(sv.contains("input logic [15:0] q_data"));
    assert!(sv.contains("input logic [1:0] q_tag"));
    assert!(sv.contains("logic [7:0] w_data"));
    assert!(sv.contains("logic [1:0] w_tag"));
}
