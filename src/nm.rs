use zbus::{proxy, zvariant::OwnedObjectPath};

pub trait Code {
    fn code(&self) -> u32;
}

macro_rules! impl_code_for {
    ($($ty: ty), *) => {
        $(impl Code for $ty {
            fn code(&self) -> u32 {
                *self as u32
            }
        })*
    };
}

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum ConnectivityState {
    Unkown = 0,
    None = 1,
    Portal = 2,
    Limited = 3,
    Full = 4,
}

impl_code_for!(ConnectivityState);

#[proxy(
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager",
    interface = "org.freedesktop.NetworkManager"
)]
pub trait NetworkManager {
    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<OwnedObjectPath>;

    fn check_connectivity(&self) -> zbus::Result<u32>;
}

pub mod connection {
    pub mod active {
        use zbus::{proxy, zvariant::OwnedObjectPath};

        #[proxy(
            default_service = "org.freedesktop.NetworkManager",
            interface = "org.freedesktop.NetworkManager.Connection.Active"
        )]
        pub trait ActiveConnection {
            #[zbus(property)]
            fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

            #[zbus(property)]
            fn id(&self) -> zbus::Result<String>;

            #[zbus(property)]
            fn r#type(&self) -> zbus::Result<String>;
        }
    }
}

pub mod device {
    use zbus::{proxy, zvariant::OwnedObjectPath};

    #[proxy(
        default_service = "org.freedesktop.NetworkManager",
        interface = "org.freedesktop.NetworkManager.Device"
    )]
    pub trait Device {
        #[zbus(property)]
        fn ip4_config(&self) -> zbus::Result<OwnedObjectPath>;

        #[zbus(property)]
        fn ip4_connectivity(&self) -> zbus::Result<u32>;

        #[zbus(property)]
        fn ip6_connectivity(&self) -> zbus::Result<u32>;
    }
}

pub mod ip4config {
    use std::collections::HashMap;

    use zbus::{proxy, zvariant::OwnedValue};

    #[proxy(
        default_service = "org.freedesktop.NetworkManager",
        interface = "org.freedesktop.NetworkManager.IP4Config"
    )]
    pub trait IP4Config {
        #[zbus(property)]
        fn address_data(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;
    }
}
