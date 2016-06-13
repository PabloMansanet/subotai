pub enum RpcType {
   Ping,
   Store,
   FindNode,
   FindValue
}

pub struct Rpc{
   rpcType : RpcType
}
