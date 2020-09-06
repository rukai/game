#[repr(u64)]
#[derive(Clone, PartialEq, Debug, ToPrimitive, FromPrimitive, EnumIter, IntoStaticStr, Serialize, Deserialize)]
pub enum TorielFireballAction {
    Spawn,
    Travel,
    Hit,
}
