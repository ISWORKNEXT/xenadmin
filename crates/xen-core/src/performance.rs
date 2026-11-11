use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDateTime};
use serde::Serialize;
use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

#[derive(Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HostPerformance {
    Ready {
        start: u64,
        end: u64,
        step: u64,
        series: Vec<MetricSeries>,
    },
    Unsupported {
        message: String,
    },
    Unavailable {
        message: String,
    },
}

#[derive(Clone, Serialize)]
pub struct MetricSeries {
    pub name: String,
    pub unit: String,
    pub points: Vec<MetricPoint>,
}
#[derive(Clone, Serialize)]
pub struct MetricPoint {
    pub timestamp: u64,
    pub value: Option<f64>,
}

pub fn parse_server_epoch(value: &Value) -> Option<u64> {
    if let Some(number) = numeric(value).filter(|n| *n >= 0.0) {
        return Some(number as u64);
    }
    let value = value.as_str()?;
    if let Ok(date) = DateTime::parse_from_rfc3339(value) {
        return u64::try_from(date.timestamp()).ok();
    }
    for pattern in ["%Y%m%dT%H:%M:%SZ", "%Y%m%dT%H:%M:%S", "%Y-%m-%dT%H:%M:%SZ"] {
        if let Ok(date) = NaiveDateTime::parse_from_str(value, pattern) {
            return u64::try_from(date.and_utc().timestamp()).ok();
        }
    }
    None
}

impl Connection {
    pub async fn host_performance(
        &self,
        host_uuid: &str,
        range_seconds: u64,
    ) -> Result<HostPerformance, ApiError> {
        let interval = range_interval(range_seconds)?;
        let reference: String = self
            .request("host.get_by_uuid", json!([self.session, host_uuid]))
            .await?;
        let host: Value = self
            .request("host.get_record", json!([self.session, reference]))
            .await?;
        if host["uuid"].as_str() != Some(host_uuid) {
            return Err(ApiError::Protocol);
        }
        let server_time: Value = match self
            .request("host.get_servertime", json!([self.session, reference]))
            .await
        {
            Ok(time) => time,
            Err(error) => {
                return Ok(HostPerformance::Unavailable {
                    message: format!("Cannot read the host clock: {error}"),
                });
            }
        };
        let Some(end) = parse_server_epoch(&server_time) else {
            return Ok(HostPerformance::Unavailable {
                message: "The host returned an unrecognized server timestamp.".into(),
            });
        };
        let start = end.saturating_sub(range_seconds);
        let address = host["address"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or(ApiError::Protocol)?;
        let transport = self
            .transport
            .for_member(address, self.settings.tls)
            .map_err(|_| ApiError::InvalidHost)?;
        let mut response=match transport.rrd_updates(&self.session,start,interval).await {
            Ok(response)=>response,
            Err(_)=>return Ok(HostPerformance::Unavailable{message:"Cannot retrieve metrics from the selected host. Check member reachability and TLS settings.".into()}),
        };
        if response.status().as_u16() == 404 {
            return Ok(HostPerformance::Unsupported {
                message: "This host does not expose the RRD updates endpoint.".into(),
            });
        }
        if !response.status().is_success() {
            return Ok(HostPerformance::Unavailable {
                message: format!(
                    "Metrics endpoint returned HTTP {}.",
                    response.status().as_u16()
                ),
            });
        }
        const MAX_BYTES: usize = 32 * 1024 * 1024;
        let mut bytes = Vec::new();
        loop {
            let chunk = match response.chunk().await {
                Ok(chunk) => chunk,
                Err(_) => {
                    return Ok(HostPerformance::Unavailable {
                        message: "The metrics download was interrupted.".into(),
                    });
                }
            };
            let Some(chunk) = chunk else { break };
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Ok(HostPerformance::Unavailable {
                    message: "The metrics response is too large. Select a shorter time range."
                        .into(),
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        let payload: Value = match serde_json::from_slice(&bytes) {
            Ok(payload) => payload,
            Err(_) => {
                return Ok(HostPerformance::Unavailable {
                    message: "The host did not return valid RRD JSON metrics.".into(),
                });
            }
        };
        let sources: Vec<Value> = self
            .request("host.get_data_sources", json!([self.session, reference]))
            .await
            .unwrap_or_default();
        let units = sources
            .into_iter()
            .filter_map(|source| {
                Some((
                    source["name_label"].as_str()?.to_owned(),
                    source["units"].as_str()?.to_owned(),
                ))
            })
            .collect();
        Ok(parse_rrd(
            &payload, "host", host_uuid, &units, start, end, interval,
        ))
    }

    pub async fn vm_performance(
        &self,
        vm_uuid: &str,
        range_seconds: u64,
    ) -> Result<HostPerformance, ApiError> {
        let interval = range_interval(range_seconds)?;
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, vm_ref]))
            .await?;
        if vm["uuid"].as_str() != Some(vm_uuid) {
            return Err(ApiError::Protocol);
        }
        if vm["is_control_domain"].as_bool() == Some(true) {
            return Err(ApiError::Xen(
                "Control domains do not have VM performance graphs.".into(),
            ));
        }

        let resident = vm["resident_on"].as_str().unwrap_or_default();
        let affinity = vm["affinity"].as_str().unwrap_or_default();
        let host_ref = if !resident.is_empty() && resident != "OpaqueRef:NULL" {
            resident.to_owned()
        } else if !affinity.is_empty() && affinity != "OpaqueRef:NULL" {
            affinity.to_owned()
        } else {
            let pools = self.records("pool").await?;
            pools
                .values()
                .next()
                .and_then(|pool| pool["master"].as_str())
                .unwrap_or_default()
                .to_owned()
        };
        if host_ref.is_empty() || host_ref == "OpaqueRef:NULL" {
            return Ok(HostPerformance::Unavailable {
                message: "The VM has no resident or affinity host for metrics.".into(),
            });
        }

        let host: Value = self
            .request("host.get_record", json!([self.session, host_ref]))
            .await?;
        let address = host["address"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(ApiError::Protocol)?;
        let server_time: Value = match self
            .request("host.get_servertime", json!([self.session, host_ref]))
            .await
        {
            Ok(time) => time,
            Err(error) => {
                return Ok(HostPerformance::Unavailable {
                    message: format!("Cannot read the host clock: {error}"),
                });
            }
        };
        let Some(end) = parse_server_epoch(&server_time) else {
            return Ok(HostPerformance::Unavailable {
                message: "The host returned an unrecognized server timestamp.".into(),
            });
        };
        let start = end.saturating_sub(range_seconds);
        let transport = self
            .transport
            .for_member(address, self.settings.tls)
            .map_err(|_| ApiError::InvalidHost)?;
        let mut response = match transport.rrd_updates(&self.session, start, interval).await {
            Ok(response) => response,
            Err(_) => {
                return Ok(HostPerformance::Unavailable {
                    message: "Cannot retrieve metrics from the selected host. Check member reachability and TLS settings.".into(),
                });
            }
        };
        if response.status().as_u16() == 404 {
            return Ok(HostPerformance::Unsupported {
                message: "This host does not expose the RRD updates endpoint.".into(),
            });
        }
        if !response.status().is_success() {
            return Ok(HostPerformance::Unavailable {
                message: format!(
                    "Metrics endpoint returned HTTP {}.",
                    response.status().as_u16()
                ),
            });
        }
        let mut bytes = Vec::new();
        const MAX_BYTES: usize = 32 * 1024 * 1024;
        loop {
            let chunk = match response.chunk().await {
                Ok(chunk) => chunk,
                Err(_) => {
                    return Ok(HostPerformance::Unavailable {
                        message: "The metrics download was interrupted.".into(),
                    });
                }
            };
            let Some(chunk) = chunk else { break };
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Ok(HostPerformance::Unavailable {
                    message: "The metrics response is too large. Select a shorter time range."
                        .into(),
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        let payload: Value = match serde_json::from_slice(&bytes) {
            Ok(payload) => payload,
            Err(_) => {
                return Ok(HostPerformance::Unavailable {
                    message: "The host did not return valid RRD JSON metrics.".into(),
                });
            }
        };
        let sources: Vec<Value> = self
            .request("VM.get_data_sources", json!([self.session, vm_ref]))
            .await
            .unwrap_or_default();
        let units = sources
            .into_iter()
            .filter_map(|source| {
                Some((
                    source["name_label"].as_str()?.to_owned(),
                    source["units"].as_str()?.to_owned(),
                ))
            })
            .collect();
        Ok(parse_rrd(
            &payload, "vm", vm_uuid, &units, start, end, interval,
        ))
    }
}

fn range_interval(range_seconds: u64) -> Result<u64, ApiError> {
    match range_seconds {
        900 => Ok(5),
        3600 => Ok(60),
        86400 => Ok(300),
        _ => Err(ApiError::Xen(
            "Select a supported performance range: 15 minutes, 1 hour or 24 hours.".into(),
        )),
    }
}

fn numeric(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| n.is_finite())
}

fn parse_rrd(
    payload: &Value,
    kind: &str,
    object_uuid: &str,
    units: &BTreeMap<String, String>,
    start: u64,
    end: u64,
    step: u64,
) -> HostPerformance {
    let (Some(legend), Some(rows)) = (
        payload["meta"]["legend"].as_array(),
        payload["data"].as_array(),
    ) else {
        return HostPerformance::Unavailable {
            message: "The metrics response is missing its legend or samples.".into(),
        };
    };
    let mut series = Vec::new();
    for (index, entry) in legend.iter().enumerate() {
        let Some(entry) = entry.as_str() else {
            continue;
        };
        let parts: Vec<_> = entry.splitn(4, ':').collect();
        if parts.len() != 4
            || !parts[0].eq_ignore_ascii_case("AVERAGE")
            || parts[1] != kind
            || parts[2] != object_uuid
        {
            continue;
        }
        let name = parts[3];
        let source_unit = units.get(name).map(String::as_str).unwrap_or("");
        let cpu = name == "cpu_avg"
            || name
                .strip_prefix("cpu")
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        let kib = name.ends_with("_kib") || source_unit == "KiB";
        let scale = if cpu {
            100.0
        } else if kib {
            1024.0
        } else {
            1.0
        };
        let unit = if cpu {
            "%"
        } else if kib {
            "B"
        } else {
            source_unit
        };
        let mut points: Vec<_> = rows
            .iter()
            .filter_map(|row| {
                let timestamp = parse_server_epoch(&row["t"])?;
                if timestamp < start || timestamp > end {
                    return None;
                }
                Some(MetricPoint {
                    timestamp,
                    value: numeric(&row["values"][index])
                        .map(|v| v * scale)
                        .filter(|v| v.is_finite()),
                })
            })
            .collect();
        points.sort_by_key(|point| point.timestamp);
        points.dedup_by_key(|point| point.timestamp);
        series.push(MetricSeries {
            name: name.into(),
            unit: unit.into(),
            points,
        });
    }
    series.sort_by(|a, b| a.name.cmp(&b.name));
    HostPerformance::Ready {
        start,
        end,
        step: numeric(&payload["meta"]["step"])
            .filter(|n| *n > 0.0)
            .map_or(step, |n| n as u64),
        series,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rrd_excludes_other_hosts_and_vms_and_preserves_unknown_gaps() {
        let payload = json!({"meta":{"step":"5","legend":["AVERAGE:host:a:cpu_avg","AVERAGE:host:b:cpu_avg","AVERAGE:vm:a:cpu0","AVERAGE:host:a:memory_free_kib"]},"data":[{"t":"100","values":["NaN",0.9,0.8,"2048"]},{"t":105,"values":["0.25",0.9,0.8,"Infinity"]}]});
        let HostPerformance::Ready { series, .. } =
            parse_rrd(&payload, "host", "a", &BTreeMap::new(), 90, 110, 5)
        else {
            panic!("Expected valid metrics")
        };
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].points[0].value, None);
        assert_eq!(series[0].points[1].value, Some(25.0));
        assert_eq!(series[0].unit, "%");
        assert_eq!(series[1].points[0].value, Some(2097152.0));
        assert_eq!(series[1].points[1].value, None);
    }
    #[test]
    fn server_clock_supports_xenapi_dates_without_using_client_timezone() {
        let expected = Some(1794186000);
        let first = parse_server_epoch(&json!("20261109T01:00:00Z"));
        assert_eq!(first, parse_server_epoch(&json!("2026-11-09T01:00:00Z")));
        assert_eq!(first, expected);
        assert_eq!(parse_server_epoch(&json!("not a timestamp")), None);
    }
    #[test]
    fn vm_series_are_selected_separately_from_host_series() {
        let payload = json!({
            "meta": {"step": "60", "legend": [
                "AVERAGE:host:a:cpu0",
                "AVERAGE:vm:a:cpu0",
                "AVERAGE:vm:a:memory_internal_free"
            ]},
            "data": [
                {"t": "100", "values": ["0.5", "0.25", "4096"]}
            ]
        });
        let HostPerformance::Ready { series, .. } =
            parse_rrd(&payload, "vm", "a", &BTreeMap::new(), 90, 110, 60)
        else {
            panic!("expected VM series");
        };
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].name, "cpu0");
        assert_eq!(series[0].unit, "%");
        assert_eq!(series[1].name, "memory_internal_free");
    }
}
