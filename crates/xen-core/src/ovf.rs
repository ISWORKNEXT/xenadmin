//! OVF envelope interchange. Device order follows VirtualHardwareSection Items.
use crate::xapi::ApiError;
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disk {
    pub id: String,
    pub file: String,
    pub capacity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Machine {
    pub name: String,
    pub vcpus: u64,
    pub memory_mib: u64,
    pub disks: Vec<Disk>,
    pub networks: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Envelope {
    pub machines: Vec<Machine>,
    pub eulas: Vec<String>,
}

pub(crate) fn error(message: impl Into<String>) -> ApiError {
    ApiError::Xen(message.into())
}
pub(crate) fn io(error: std::io::Error) -> ApiError {
    self::error(format!("Appliance file operation failed: {error}"))
}
fn attr<'a>(n: roxmltree::Node<'a, 'a>, key: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == key).map(|a| a.value())
}
fn child(n: roxmltree::Node<'_, '_>, key: &str) -> String {
    n.children()
        .find(|n| n.tag_name().name() == key)
        .and_then(|n| n.text())
        .unwrap_or_default()
        .into()
}
fn quantity(value: &str, units: &str) -> Result<u64, ApiError> {
    let value: u64 = value.parse().map_err(|_| error("Invalid OVF quantity."))?;
    let multiplier = match units.trim().to_ascii_lowercase().as_str() {
        "" | "byte" | "bytes" => 1,
        "kilobyte" | "kilobytes" | "byte * 2^10" => 1024,
        "megabyte" | "megabytes" | "byte * 2^20" => 1024 * 1024,
        "gigabyte" | "gigabytes" | "byte * 2^30" => 1024 * 1024 * 1024,
        _ => return Err(error(format!("Unsupported OVF allocation units: {units}"))),
    };
    value
        .checked_mul(multiplier)
        .ok_or_else(|| error("OVF quantity overflow."))
}
pub fn relative_file(root: &Path, name: &str) -> Result<PathBuf, ApiError> {
    let path = Path::new(name);
    if name.contains(['\\', ':'])
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        || path.as_os_str().is_empty()
    {
        return Err(error("Appliance references must be relative local files."));
    }
    Ok(root.join(path))
}
pub fn parse(xml: &str) -> Result<Envelope, ApiError> {
    let doc =
        roxmltree::Document::parse(xml).map_err(|e| error(format!("Invalid OVF XML: {e}")))?;
    if doc.root_element().tag_name().name() != "Envelope" {
        return Err(error("Select an OVF Envelope."));
    }
    let mut files = HashMap::new();
    let mut disks = HashMap::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "File" => {
                let id = attr(n, "id").ok_or_else(|| error("OVF File has no ID."))?;
                let href = attr(n, "href").ok_or_else(|| error("OVF File has no href."))?;
                relative_file(Path::new("."), href)?;
                files.insert(id.to_owned(), href.to_owned());
            }
            "Disk" => {
                let id = attr(n, "diskId").ok_or_else(|| error("OVF Disk has no ID."))?;
                let file = attr(n, "fileRef").unwrap_or_default();
                let capacity = quantity(
                    attr(n, "capacity").unwrap_or("0"),
                    attr(n, "capacityAllocationUnits").unwrap_or("byte"),
                )?;
                disks.insert(id.to_owned(), (file.to_owned(), capacity));
            }
            _ => {}
        }
    }
    let mut envelope = Envelope::default();
    for n in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "EulaSection")
    {
        envelope.eulas.push(child(n, "License"));
    }
    for n in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "VirtualSystem")
    {
        let mut vm = Machine {
            name: child(n, "Name"),
            vcpus: 1,
            memory_mib: 256,
            disks: Vec::new(),
            networks: Vec::new(),
        };
        if vm.name.is_empty() {
            vm.name = attr(n, "id").unwrap_or("Imported VM").into();
        }
        for item in n.descendants().filter(|n| n.tag_name().name() == "Item") {
            match child(item, "ResourceType").as_str() {
                "3" => vm.vcpus = quantity(&child(item, "VirtualQuantity"), "")?,
                "4" => {
                    vm.memory_mib = quantity(
                        &child(item, "VirtualQuantity"),
                        &child(item, "AllocationUnits"),
                    )? / (1024 * 1024)
                }
                "10" => vm.networks.push(child(item, "Connection")),
                "17" | "19" => {
                    let host = child(item, "HostResource");
                    let id = host.rsplit('/').next().unwrap_or_default();
                    let (file, capacity) = disks
                        .get(id)
                        .ok_or_else(|| error("Unknown OVF disk reference."))?;
                    vm.disks.push(Disk {
                        id: id.into(),
                        file: files
                            .get(file)
                            .cloned()
                            .ok_or_else(|| error("Unknown OVF file reference."))?,
                        capacity: *capacity,
                    });
                }
                _ => {}
            }
        }
        if vm.vcpus == 0 || vm.memory_mib == 0 {
            return Err(error("OVF CPU and memory quantities must be positive."));
        }
        envelope.machines.push(vm);
    }
    if envelope.machines.is_empty() {
        return Err(error("The OVF contains no virtual machines."));
    }
    Ok(envelope)
}
fn esc(value: &str) -> String {
    quick_xml::escape::escape(value).into_owned()
}
pub fn write(envelope: &Envelope) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Envelope xmlns=\"http://schemas.dmtf.org/ovf/envelope/1\" xmlns:ovf=\"http://schemas.dmtf.org/ovf/envelope/1\" xmlns:rasd=\"http://schemas.dmtf.org/wbem/wscim/1/cim-schema/2/CIM_ResourceAllocationSettingData\" xmlns:vssd=\"http://schemas.dmtf.org/wbem/wscim/1/cim-schema/2/CIM_VirtualSystemSettingData\"><References>",
    );
    for vm in &envelope.machines {
        for disk in &vm.disks {
            xml += &format!(
                "<File ovf:id=\"{}\" ovf:href=\"{}\"/>",
                esc(&disk.id),
                esc(&disk.file)
            );
        }
    }
    xml += "</References><DiskSection><Info>Virtual disks</Info>";
    for vm in &envelope.machines {
        for disk in &vm.disks {
            xml += &format!(
                "<Disk ovf:diskId=\"{}\" ovf:fileRef=\"{}\" ovf:capacity=\"{}\" ovf:format=\"http://www.microsoft.com/technet/virtualserver/downloads/vhdspec.mspx\"/>",
                esc(&disk.id),
                esc(&disk.id),
                disk.capacity
            );
        }
    }
    xml += "</DiskSection><NetworkSection><Info>Logical networks</Info>";
    let networks: std::collections::BTreeSet<_> = envelope
        .machines
        .iter()
        .flat_map(|vm| &vm.networks)
        .collect();
    for network in networks {
        xml += &format!("<Network ovf:name=\"{}\"/>", esc(network));
    }
    xml += "</NetworkSection><VirtualSystemCollection ovf:id=\"appliance\"><Info>Virtual appliance</Info>";
    for (index, vm) in envelope.machines.iter().enumerate() {
        xml += &format!(
            "<VirtualSystem ovf:id=\"vm{index}\"><Info>Virtual machine</Info><Name>{}</Name><VirtualHardwareSection><Info>Virtual hardware</Info><System><vssd:VirtualSystemIdentifier>{}</vssd:VirtualSystemIdentifier><vssd:VirtualSystemType>xen-3.0</vssd:VirtualSystemType></System>",
            esc(&vm.name),
            esc(&vm.name)
        );
        let mut item = |id: usize, kind: u64, extra: String| {
            xml += &format!(
                "<Item><rasd:InstanceID>{id}</rasd:InstanceID><rasd:ResourceType>{kind}</rasd:ResourceType>{extra}</Item>"
            );
        };
        item(
            0,
            3,
            format!("<rasd:VirtualQuantity>{}</rasd:VirtualQuantity>", vm.vcpus),
        );
        item(
            1,
            4,
            format!(
                "<rasd:AllocationUnits>byte * 2^20</rasd:AllocationUnits><rasd:VirtualQuantity>{}</rasd:VirtualQuantity>",
                vm.memory_mib
            ),
        );
        item(2, 6, "<rasd:Address>0</rasd:Address>".into());
        for (i, disk) in vm.disks.iter().enumerate() {
            item(
                i + 3,
                17,
                format!(
                    "<rasd:Parent>2</rasd:Parent><rasd:AddressOnParent>{i}</rasd:AddressOnParent><rasd:HostResource>ovf:/disk/{}</rasd:HostResource>",
                    esc(&disk.id)
                ),
            );
        }
        for (i, network) in vm.networks.iter().enumerate() {
            item(
                i + 3 + vm.disks.len(),
                10,
                format!("<rasd:Connection>{}</rasd:Connection>", esc(network)),
            );
        }
        xml += "</VirtualHardwareSection></VirtualSystem>";
    }
    xml += "</VirtualSystemCollection></Envelope>";
    xml
}
/// Footer cookie, checksum and current virtual size, independent of the file's extension.
pub fn vhd_size(reader: &mut (impl Read + Seek)) -> Result<Option<u64>, ApiError> {
    let length = reader.seek(SeekFrom::End(0)).map_err(io)?;
    if length < 512 {
        return Ok(None);
    }
    reader.seek(SeekFrom::End(-512)).map_err(io)?;
    let mut footer = [0u8; 512];
    reader.read_exact(&mut footer).map_err(io)?;
    if &footer[..8] != b"conectix" {
        return Ok(None);
    }
    let expected = u32::from_be_bytes(footer[64..68].try_into().unwrap());
    let actual = !footer
        .iter()
        .enumerate()
        .filter(|(i, _)| !(64..68).contains(i))
        .map(|(_, b)| u32::from(*b))
        .sum::<u32>();
    if expected != actual {
        return Err(error("Invalid VHD footer checksum."));
    }
    let size = u64::from_be_bytes(footer[48..56].try_into().unwrap());
    if size == 0 {
        return Err(error("VHD has zero virtual capacity."));
    }
    Ok(Some(size))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ovf_round_trip() {
        let e = Envelope {
            machines: vec![Machine {
                name: "VM & <test>".into(),
                vcpus: 2,
                memory_mib: 512,
                disks: vec![Disk {
                    id: "disk0".into(),
                    file: "disk0.vhd".into(),
                    capacity: 1073741824,
                }],
                networks: vec!["Network & 0".into(), "Network 1".into()],
            }],
            eulas: vec![],
        };
        assert_eq!(parse(&write(&e)).unwrap(), e);
    }
    #[test]
    fn vhd_footer_detection() {
        let mut footer = [0u8; 512];
        footer[..8].copy_from_slice(b"conectix");
        footer[48..56].copy_from_slice(&67108864u64.to_be_bytes());
        let checksum = !footer.iter().map(|b| u32::from(*b)).sum::<u32>();
        footer[64..68].copy_from_slice(&checksum.to_be_bytes());
        assert_eq!(
            vhd_size(&mut std::io::Cursor::new(footer)).unwrap(),
            Some(67108864)
        );
        footer[50] ^= 1;
        assert!(vhd_size(&mut std::io::Cursor::new(footer)).is_err());
        assert_eq!(
            vhd_size(&mut std::io::Cursor::new([0u8; 1024])).unwrap(),
            None
        );
    }
}
