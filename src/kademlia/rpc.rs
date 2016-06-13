pub enum RpcType {
   Ping,
   Store,
   FindNode,
   FindValue
}

pub struct Rpc{
   rpc_type : RpcType
}
