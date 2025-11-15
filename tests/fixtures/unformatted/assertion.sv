module assertion_demo;
always_ff @(posedge clk) begin
assert property (@(posedge clk) req |-> ack)
  pass_count <= pass_count + 1;
else
  fail_count <= fail_count + 1;
end
endmodule

