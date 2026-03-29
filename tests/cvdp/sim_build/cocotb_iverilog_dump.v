module cocotb_iverilog_dump();
initial begin
    string dumpfile_path;    if ($value$plusargs("dumpfile_path=%s", dumpfile_path)) begin
        $dumpfile(dumpfile_path);
    end else begin
        $dumpfile("/Users/shuqingzhao/github/arch-com/tests/cvdp/sim_build/binary_search_tree_sort.fst");
    end
    $dumpvars(0, binary_search_tree_sort);
end
endmodule
