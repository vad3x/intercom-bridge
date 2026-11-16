//! Pure endpoint metadata grouping for the Matter node descriptor.

extern crate alloc;

use alloc::{vec, vec::Vec};
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter::dm::clusters::identify;
use rs_matter::dm::{Cluster, DeviceType};

#[derive(Clone)]
pub(crate) struct ClusterRegistrationMeta {
    pub(crate) id: u16,
    pub(crate) device_type: DeviceType,
    pub(crate) cluster: Cluster<'static>,
    pub(crate) client_clusters: &'static [rs_matter::dm::ClusterId],
}

pub(crate) struct GroupedEndpointMeta {
    pub(crate) id: u16,
    pub(crate) device_types: [DeviceType; 1],
    pub(crate) server_clusters: Vec<Cluster<'static>>,
    pub(crate) client_clusters: Vec<rs_matter::dm::ClusterId>,
}

pub(crate) fn group_endpoint_clusters(
    metas: &[ClusterRegistrationMeta],
) -> Vec<GroupedEndpointMeta> {
    let mut grouped: Vec<GroupedEndpointMeta> = Vec::new();
    for meta in metas {
        if let Some(existing) = grouped.iter_mut().find(|g| g.id == meta.id) {
            if !existing
                .server_clusters
                .iter()
                .any(|c| c.id == meta.cluster.id)
            {
                existing.server_clusters.push(meta.cluster.clone());
            }
            for &client_id in meta.client_clusters {
                if !existing.client_clusters.contains(&client_id) {
                    existing.client_clusters.push(client_id);
                }
            }
        } else {
            let mut client_clusters = Vec::new();
            for &client_id in meta.client_clusters {
                if !client_clusters.contains(&client_id) {
                    client_clusters.push(client_id);
                }
            }
            grouped.push(GroupedEndpointMeta {
                id: meta.id,
                device_types: [meta.device_type],
                server_clusters: vec![
                    desc::DescHandler::CLUSTER,
                    identify::CLUSTER,
                    meta.cluster.clone(),
                ],
                client_clusters,
            });
        }
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs_matter::dm::clusters::app::audio_av_stream::{
        AudioAvStreamHandler, AudioAvStreamHooks, AudioStream, CamAvError,
    };
    use rs_matter::dm::clusters::decl::{
        chime, door_lock, switch, web_rtc_transport_provider, web_rtc_transport_requestor,
    };
    use rs_matter::dm::devices::DEV_TYPE_AUDIO_DOORBELL;

    const DEV_TYPE_DOOR_LOCK: DeviceType = DeviceType {
        dtype: 0x000A,
        drev: 3,
    };

    struct DummyAudioHooksMock;
    impl AudioAvStreamHooks for DummyAudioHooksMock {
        async fn on_audio_allocated(&self, _s: &AudioStream) -> Result<(), CamAvError> {
            Ok(())
        }
        async fn on_audio_deallocated(&self, _id: u16) -> Result<(), CamAvError> {
            Ok(())
        }
        async fn on_volume_or_mute_changed(
            &self,
            _: bool,
            _: u8,
            _: bool,
            _: u8,
        ) -> Result<(), CamAvError> {
            Ok(())
        }
    }

    #[test]
    fn groups_multi_cluster_audio_doorbell_and_preserves_client_clusters() {
        // Arrange
        let metas = [
            ClusterRegistrationMeta {
                id: 1,
                device_type: DEV_TYPE_AUDIO_DOORBELL,
                cluster: switch::FULL_CLUSTER,
                client_clusters: &[],
            },
            ClusterRegistrationMeta {
                id: 1,
                device_type: DEV_TYPE_AUDIO_DOORBELL,
                cluster: AudioAvStreamHandler::<DummyAudioHooksMock, 2>::CLUSTER,
                client_clusters: &[],
            },
            ClusterRegistrationMeta {
                id: 1,
                device_type: DEV_TYPE_AUDIO_DOORBELL,
                cluster: web_rtc_transport_provider::FULL_CLUSTER,
                client_clusters: &[web_rtc_transport_requestor::FULL_CLUSTER.id],
            },
            ClusterRegistrationMeta {
                id: 1,
                device_type: DEV_TYPE_AUDIO_DOORBELL,
                cluster: chime::FULL_CLUSTER,
                client_clusters: &[],
            },
            ClusterRegistrationMeta {
                id: 2,
                device_type: DEV_TYPE_DOOR_LOCK,
                cluster: door_lock::FULL_CLUSTER,
                client_clusters: &[],
            },
        ];

        // Act
        let grouped = group_endpoint_clusters(&metas);

        // Assert
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].id, 1);
        assert_eq!(grouped[0].device_types.len(), 1);
        assert_eq!(
            grouped[0].device_types[0].dtype,
            DEV_TYPE_AUDIO_DOORBELL.dtype
        );
        assert_eq!(
            grouped[0].device_types[0].drev,
            DEV_TYPE_AUDIO_DOORBELL.drev
        );
        // Descriptor (0x001D) + Identify (0x0003) + Switch (0x003B) + AVSM (0x0551) + WebRTCProv (0x0553) + Chime (0x0556)
        assert_eq!(grouped[0].server_clusters.len(), 6);
        assert_eq!(
            grouped[0].server_clusters[0].id,
            desc::DescHandler::CLUSTER.id
        );
        assert_eq!(grouped[0].server_clusters[1].id, identify::CLUSTER.id);
        assert_eq!(
            grouped[0].client_clusters,
            vec![web_rtc_transport_requestor::FULL_CLUSTER.id]
        );

        assert_eq!(grouped[1].id, 2);
        assert_eq!(grouped[1].device_types.len(), 1);
        assert_eq!(grouped[1].device_types[0].dtype, DEV_TYPE_DOOR_LOCK.dtype);
        assert_eq!(grouped[1].device_types[0].drev, DEV_TYPE_DOOR_LOCK.drev);
        // Descriptor (0x001D) + Identify (0x0003) + DoorLock (0x0101)
        assert_eq!(grouped[1].server_clusters.len(), 3);
        assert!(grouped[1].client_clusters.is_empty());
    }

    #[test]
    fn deduplicates_server_and_client_clusters_on_an_existing_endpoint() {
        // Arrange
        let meta = ClusterRegistrationMeta {
            id: 1,
            device_type: DEV_TYPE_AUDIO_DOORBELL,
            cluster: chime::FULL_CLUSTER,
            client_clusters: &[
                web_rtc_transport_requestor::FULL_CLUSTER.id,
                web_rtc_transport_requestor::FULL_CLUSTER.id,
            ],
        };
        let metas = [meta.clone(), meta];

        // Act
        let grouped = group_endpoint_clusters(&metas);

        // Assert
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].server_clusters.len(), 3);
        assert_eq!(
            grouped[0].client_clusters,
            vec![web_rtc_transport_requestor::FULL_CLUSTER.id]
        );
    }
}
